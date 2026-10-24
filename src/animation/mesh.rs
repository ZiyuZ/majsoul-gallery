//! Mesh extraction with buffers sized from the attachment's float count.
use rusty_spine::{c::c_void, controller::SkeletonController, *};

pub struct Mesh {
    pub vertices: Vec<[f32; 2]>,
    pub uvs: Vec<[f32; 2]>,
    pub indices: Vec<u16>,
    pub color: Color,
    pub dark_color: Color,
    pub blend_mode: BlendMode,
    pub premultiplied_alpha: bool,
    pub attachment_renderer_object: Option<*const c_void>,
}
pub fn collect(controller: &mut SkeletonController) -> Vec<Mesh> {
    let mut result = Vec::new();
    let pma = controller.settings.premultiplied_alpha;
    let skeleton = &mut controller.skeleton;
    let clipper = &mut controller.clipper;
    for index in 0..skeleton.slots_count() {
        let Some(slot) = skeleton.draw_order_at_index(index) else {
            continue;
        };
        if !slot.bone().active() {
            clipper.clip_end(&slot);
            continue;
        }
        if let Some(clip) = slot.attachment().and_then(|a| a.as_clipping()) {
            clipper.clip_start(&slot, &clip);
            continue;
        }
        let (mut vertices, mut uvs, mut indices, mut color, texture) =
            if let Some(mesh) = slot.attachment().and_then(|a| a.as_mesh()) {
                let count = mesh.world_vertices_length() as usize;
                if count == 0 || count > 1_000_000 || !count.is_multiple_of(2) {
                    continue;
                }
                let mut world = vec![0.; count];
                // `count` is a number of floats, with two floats per vertex.
                unsafe {
                    c::spMeshAttachment_updateRegion(mesh.c_ptr());
                    mesh.compute_world_vertices(&slot, 0, count as i32, &mut world, 0, 2);
                }
                let vertices = world.as_chunks::<2>().0.to_vec();
                let uv_ptr = unsafe { (*mesh.c_ptr()).uvs };
                let index_count = mesh.triangles_count() as usize;
                if uv_ptr.is_null() || index_count == 0 {
                    continue;
                }
                let uvs = unsafe { std::slice::from_raw_parts(uv_ptr, count) }
                    .as_chunks::<2>()
                    .0
                    .to_vec();
                let indices =
                    unsafe { std::slice::from_raw_parts(mesh.triangles(), index_count) }.to_vec();
                let mut renderer = mesh.renderer_object();
                let texture = unsafe { renderer.get_atlas_region() }
                    .map(|r| unsafe { (*r.page().c_ptr()).rendererObject.cast_const() });
                (vertices, uvs, indices, mesh.color(), texture)
            } else if let Some(region) = slot.attachment().and_then(|a| a.as_region()) {
                let mut world = vec![0.; 8];
                unsafe {
                    region.compute_world_vertices(&slot, &mut world, 0, 2);
                }
                let vertices = world.as_chunks::<2>().0.to_vec();
                let uvs = region.uvs().as_chunks::<2>().0.to_vec();
                let mut renderer = region.renderer_object();
                let texture = unsafe { renderer.get_atlas_region() }
                    .map(|r| unsafe { (*r.page().c_ptr()).rendererObject.cast_const() });
                (
                    vertices,
                    uvs,
                    vec![0, 1, 2, 2, 3, 0],
                    region.color(),
                    texture,
                )
            } else {
                clipper.clip_end(&slot);
                continue;
            };
        if indices.iter().any(|&i| i as usize >= vertices.len()) {
            clipper.clip_end(&slot);
            continue;
        }
        if clipper.is_clipping() {
            unsafe {
                c::spSkeletonClipping_clipTriangles(
                    clipper.c_ptr(),
                    vertices.as_mut_ptr().cast(),
                    (vertices.len() * 2) as i32,
                    indices.as_mut_ptr(),
                    indices.len() as i32,
                    uvs.as_mut_ptr().cast(),
                    2,
                );
                let clip = &*clipper.c_ptr();
                let points = &*clip.clippedVertices;
                let texcoords = &*clip.clippedUVs;
                let triangles = &*clip.clippedTriangles;
                vertices = if points.size > 0 {
                    std::slice::from_raw_parts(points.items, points.size as usize)
                        .as_chunks::<2>()
                        .0
                        .to_vec()
                } else {
                    Vec::new()
                };
                uvs = if texcoords.size > 0 {
                    std::slice::from_raw_parts(texcoords.items, texcoords.size as usize)
                        .as_chunks::<2>()
                        .0
                        .to_vec()
                } else {
                    Vec::new()
                };
                indices = if triangles.size > 0 {
                    std::slice::from_raw_parts(triangles.items, triangles.size as usize).to_vec()
                } else {
                    Vec::new()
                };
            }
        }
        color *= slot.color() * skeleton.color();
        let mut dark = slot.dark_color().unwrap_or_default();
        if pma {
            color.premultiply_alpha();
            dark *= color.a;
            dark.a = 1.;
        } else {
            dark.a = 0.;
        }
        result.push(Mesh {
            vertices,
            uvs,
            indices,
            color,
            dark_color: dark,
            blend_mode: slot.data().blend_mode(),
            premultiplied_alpha: pma,
            attachment_renderer_object: texture.filter(|p| !p.is_null()),
        });
        clipper.clip_end(&slot);
    }
    clipper.clip_end2();
    result
}
