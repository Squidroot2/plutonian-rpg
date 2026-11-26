use gltf::{
    Accessor, Gltf, Semantic,
    accessor::{DataType, Dimensions, Iter},
    animation::{Property as TargetProperty, util::ReadOutputs},
    buffer, image, scene,
};
use plutonian_rpg::asset_check::{Animation, Bounds, Model, Node, Property, Track, Transform};

const HEADER_LEN: usize = 12;

pub fn parse(bytes: &[u8]) -> Result<Model, String> {
    if bytes.len() < HEADER_LEN || &bytes[..4] != b"glTF" {
        return Err("invalid GLB header".into());
    }
    let declared_length = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if usize::try_from(declared_length).ok() != Some(bytes.len()) {
        return Err(format!(
            "invalid GLB length {declared_length} (found {} bytes)",
            bytes.len()
        ));
    }
    let gltf = Gltf::from_slice(bytes).map_err(|error| format!("invalid GLB: {error}"))?;
    let data = |buffer: buffer::Buffer<'_>| match buffer.source() {
        buffer::Source::Bin => gltf.blob.as_deref(),
        buffer::Source::Uri(_) => None,
    };
    let mut decode_failures = Vec::new();
    let meshes = gltf
        .meshes()
        .map(|mesh| {
            mesh.primitives()
                .filter_map(|primitive| match primitive_bounds(&primitive, data) {
                    Ok(bounds) => Some(bounds),
                    Err(error) => {
                        decode_failures.push(format!(
                            "mesh {} primitive {}: {error}",
                            mesh.index(),
                            primitive.index()
                        ));
                        None
                    }
                })
                .collect()
        })
        .collect();
    let animations = gltf
        .animations()
        .map(|clip| read_animation(clip, data, &mut decode_failures))
        .collect();
    let nodes = gltf
        .nodes()
        .map(|node| Node {
            name: node.name().map(str::to_owned),
            mesh: node.mesh().map(|mesh| mesh.index()),
            children: node.children().map(|child| child.index()).collect(),
            transform: match node.transform() {
                scene::Transform::Matrix { matrix } => Transform::Matrix(matrix),
                scene::Transform::Decomposed {
                    translation,
                    rotation,
                    scale,
                } => Transform::Trs {
                    translation,
                    rotation,
                    scale,
                },
            },
        })
        .collect();
    Ok(Model {
        scenes: gltf
            .scenes()
            .map(|scene| scene.nodes().map(|node| node.index()).collect())
            .collect(),
        meshes,
        nodes,
        has_skin: gltf.skins().next().is_some(),
        has_external_images: gltf.images().any(|image| match image.source() {
            image::Source::Uri { uri, .. } => !uri.starts_with("data:"),
            image::Source::View { .. } => false,
        }),
        has_external_buffers: gltf
            .buffers()
            .any(|buffer| matches!(buffer.source(), buffer::Source::Uri(_))),
        animations,
        decode_failures,
    })
}

fn primitive_bounds<'data>(
    primitive: &gltf::Primitive<'_>,
    data: impl Clone + Fn(buffer::Buffer<'_>) -> Option<&'data [u8]>,
) -> Result<Bounds, &'static str> {
    let accessor = primitive
        .get(&Semantic::Positions)
        .ok_or("missing POSITION accessor")?;
    if !readable(&accessor, Dimensions::Vec3) || accessor.data_type() != DataType::F32 {
        return Err("invalid POSITION accessor");
    }
    let positions = primitive
        .reader(data)
        .read_positions()
        .ok_or("unreadable POSITION samples")?;
    let min: [f32; 3] = accessor
        .min()
        .and_then(|value| serde_json::from_value(value).ok())
        .ok_or("missing or invalid POSITION minimum")?;
    let max: [f32; 3] = accessor
        .max()
        .and_then(|value| serde_json::from_value(value).ok())
        .ok_or("missing or invalid POSITION maximum")?;
    let mut actual_min = [f32::INFINITY; 3];
    let mut actual_max = [f32::NEG_INFINITY; 3];
    for position in positions {
        for axis in 0..3 {
            if !position[axis].is_finite() {
                return Err("non-finite POSITION sample");
            }
            actual_min[axis] = actual_min[axis].min(position[axis]);
            actual_max[axis] = actual_max[axis].max(position[axis]);
        }
    }
    if min != actual_min || max != actual_max {
        return Err("POSITION bounds disagree with vertex samples");
    }
    Ok(Bounds { min, max })
}

/// The glTF readers assume nonempty, correctly typed accessor ranges.
fn readable(accessor: &Accessor<'_>, dimensions: Dimensions) -> bool {
    let count = accessor.count();
    let size = accessor.size();
    if count == 0 || accessor.dimensions() != dimensions {
        return false;
    }
    if let Some(view) = accessor.view()
        && !range_fits(&view, accessor.offset(), count, size)
    {
        return false;
    }
    if let Some(sparse) = accessor.sparse() {
        let indices = sparse.indices();
        let values = sparse.values();
        let sparse_count = sparse.count();
        return sparse_count > 0
            && sparse_count <= count
            && range_fits(
                &indices.view(),
                indices.offset(),
                sparse_count,
                indices.index_type().size(),
            )
            && range_fits(&values.view(), values.offset(), sparse_count, size);
    }
    accessor.view().is_some()
}

fn range_fits(view: &buffer::View<'_>, offset: usize, count: usize, size: usize) -> bool {
    let stride = view.stride().unwrap_or(size);
    stride >= size
        && view.offset().checked_add(view.length()).is_some()
        && (count - 1)
            .checked_mul(stride)
            .and_then(|length| length.checked_add(size))
            .and_then(|length| offset.checked_add(length))
            .is_some_and(|end| end <= view.length())
}

fn read_animation<'data>(
    clip: gltf::Animation<'_>,
    data: impl Clone + Fn(buffer::Buffer<'_>) -> Option<&'data [u8]>,
    failures: &mut Vec<String>,
) -> Animation {
    let name = clip.name().unwrap_or_default().to_owned();
    let timelines: Vec<_> = clip
        .samplers()
        .map(|sampler| {
            read_timeline(sampler.input(), data.clone())
                .inspect_err(|error| {
                    failures.push(format!(
                        "clip {name:?} sampler {}: {error}",
                        sampler.index()
                    ));
                })
                .ok()
        })
        .collect();
    let duration_s = timelines
        .iter()
        .flatten()
        .filter_map(|times| times.last().copied())
        .reduce(f32::max);
    let mut tracks = Vec::new();
    for channel in clip.channels() {
        let sampler = channel.sampler();
        let Some(times) = &timelines[sampler.index()] else {
            continue;
        };
        let reader = channel.reader(data.clone());
        let output = sampler.output();
        let expected_dimensions = match channel.target().property() {
            TargetProperty::Translation | TargetProperty::Scale => Dimensions::Vec3,
            TargetProperty::Rotation => Dimensions::Vec4,
            TargetProperty::MorphTargetWeights => Dimensions::Scalar,
        };
        if !readable(&output, expected_dimensions)
            || output.data_type() == DataType::U32
            || expected_dimensions == Dimensions::Vec3 && output.data_type() != DataType::F32
        {
            failures.push(format!(
                "clip {name:?} channel {} has an invalid pose accessor",
                channel.index()
            ));
            continue;
        }
        let (property, mut values): (_, Vec<_>) = match reader.read_outputs() {
            Some(ReadOutputs::Translations(values)) => {
                (Property::Translation, values.map(pad).collect())
            }
            Some(ReadOutputs::Rotations(values)) => {
                (Property::Rotation, values.into_f32().collect())
            }
            Some(ReadOutputs::Scales(values)) => (Property::Scale, values.map(pad).collect()),
            Some(ReadOutputs::MorphTargetWeights(_)) => continue,
            None => {
                failures.push(format!(
                    "clip {name:?} channel {} has unreadable output samples",
                    channel.index()
                ));
                continue;
            }
        };
        if sampler.interpolation() == gltf::animation::Interpolation::CubicSpline {
            // Cubic sampler outputs are triplets of in-tangent, value, and out-tangent.
            values = values.into_iter().skip(1).step_by(3).collect();
        }
        if values.len() != times.len() || values.iter().flatten().any(|value| !value.is_finite()) {
            failures.push(format!(
                "clip {name:?} channel {} has invalid output samples",
                channel.index()
            ));
            continue;
        }
        tracks.push(Track {
            node: channel.target().node().index(),
            property,
            times: times.clone(),
            values,
        });
    }
    Animation {
        name,
        duration_s,
        tracks,
    }
}

fn read_timeline<'data>(
    input: Accessor<'_>,
    data: impl Clone + Fn(buffer::Buffer<'_>) -> Option<&'data [u8]>,
) -> Result<Vec<f32>, &'static str> {
    if !readable(&input, Dimensions::Scalar) || input.data_type() != DataType::F32 {
        return Err("invalid time accessor");
    }
    let declared_end = input
        .max()
        .and_then(|value| serde_json::from_value::<[f32; 1]>(value).ok());
    let times: Vec<f32> = Iter::new(input, data)
        .ok_or("unreadable input samples")?
        .collect();
    let end = times.last().copied().ok_or("no input samples")?;
    if declared_end != Some([end])
        || !end.is_finite()
        || !times.windows(2).all(|pair| pair[0] < pair[1])
    {
        return Err("invalid sample times or duration bounds");
    }
    Ok(times)
}

fn pad([x, y, z]: [f32; 3]) -> [f32; 4] {
    [x, y, z, 0.0]
}
