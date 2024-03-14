use awsm_web::{
    dom::resize::ResizeObserver,
    tick::{MainLoop, MainLoopOptions, Raf},
    webgl::{
        get_webgl_context_2, BeginMode, BlendFactor, BufferData, BufferMask, BufferTarget,
        BufferUsage, DataType, GlToggle, ResizeStrategy, ShaderType, WebGl2Renderer,
        WebGlContextOptions,
    },
};
use glam::{EulerRot, Mat4, Quat};

use super::{camera::Camera, state::*};
use crate::{page::home::app::controls::PlayState, prelude::*};
use rand::prelude::*;
use num_traits::ToPrimitive;

const SHADER_COMMON_CAMERA: &'static str = include_str!("./shader/camera.glsl");
const SHADER_COMMON_MATH: &'static str = include_str!("./shader/math.glsl");
const SHADER_VERTEX_QUAD_UNIT: &'static str = include_str!("./shader/quad-unit.vert");
const SHADER_VERTEX_SPHERE_UNIT: &'static str = include_str!("./shader/sphere-unit.vert");
const SHADER_FRAGMENT_UNLIT_DIFFUSE: &'static str = include_str!("./shader/unlit-diffuse.frag");
const SHADER_FRAGMENT_MESH: &'static str = include_str!("./shader/mesh.frag");

impl Graph {
    pub fn start_render_loop(self: Rc<Self>, canvas: &web_sys::HtmlCanvasElement) -> Result<()> {
        let state = self;

        let gl = get_webgl_context_2(
            canvas,
            Some(&WebGlContextOptions {
                alpha: false,
                antialias: false,
                ..WebGlContextOptions::default()
            }),
        )?;

        let mut gl = WebGl2Renderer::new(gl)?;

        *state.camera.borrow_mut() = Some(Camera::new(&mut gl)?);
        *state.shaders.borrow_mut() = Some(compile_shaders(&mut gl)?);
        *state.geometry.borrow_mut() = Some(create_geometry(&mut gl)?);
        //gl.hardcoded_ubo_locations.insert("ubo_lights".to_string(), 1);

        *state.gl.borrow_mut() = Some(gl);

        let main_loop_opts = MainLoopOptions::default();
        let mut main_loop = MainLoop::new(
            main_loop_opts,
            |_timestamp, _delta| {},
            clone!(state => move |delta| {
                state.animate(delta).unwrap();
            }),
            clone!(state => move |interpolation| {
                state.render_tick(interpolation).unwrap();
            }),
            |fps, abort| {},
        );

        *state.raf.borrow_mut() = Some(Raf::new({
            move |ts| {
                main_loop.tick(ts);
            }
        }));

        let resize_observer = ResizeObserver::new(
            clone!(state => move |entries| {
                let entry = entries.get(0).unwrap_ext();
                let rect = &entry.content_rect;
                state.handle_resize(rect);
            }),
            None,
        );

        resize_observer.observe(&canvas);

        *state.resize_observer.borrow_mut() = Some(resize_observer);

        state.meshes.borrow_mut().push(state.new_mesh()?);

        Ok(())
    }

    pub fn animate(&self, delta: f64) -> Result<()> {
        if self.controls.play_state.get() == PlayState::Play {
            let delta = delta.to_f32().unwrap_or_default();
            let mut meshes = self.meshes.borrow_mut();
            for mesh in meshes.iter_mut() {
                mesh.animate(delta);
            }
        }
        //TODO
        Ok(())
    }

    pub fn render_tick(&self, _interpolation: f64) -> Result<()> {
        let mut gl = self.gl.borrow_mut();
        let mut gl = gl.as_mut().unwrap();
        let shaders = self.shaders.borrow();
        let shaders = shaders.as_ref().unwrap();
        let meshes = &*self.meshes.borrow();

        // camera ubo
        self.camera
            .borrow_mut()
            .as_mut()
            .unwrap()
            .update_ubo(&mut gl)?;

        // global state
        gl.set_depth_mask(true);
        gl.toggle(GlToggle::DepthTest, true);
        gl.toggle(GlToggle::Blend, true);
        gl.set_blend_func(BlendFactor::SrcAlpha, BlendFactor::OneMinusSrcAlpha);
        gl.set_clear_color(0.6, 0.6, 0.6, 1.0);
        gl.clear(&[BufferMask::ColorBufferBit, BufferMask::DepthBufferBit]);

        // draw
        let mut transform = [0.0; 16];
        for mesh in meshes {
            gl.activate_program(mesh.program_id)?;
            gl.activate_vertex_array(mesh.vao_id)?;
            mesh.get_transform().write_cols_to_slice(&mut transform);
            gl.upload_uniform_mat_4_name("u_model", &transform)?;
            gl.draw_elements(
                BeginMode::Triangles,
                mesh.index_count,
                DataType::UnsignedInt,
                0,
            );
        }

        Ok(())
    }

    pub fn handle_resize(&self, rect: &web_sys::DomRectReadOnly) {
        let mut gl = self.gl.borrow_mut();
        let mut gl = gl.as_mut().unwrap();
        let strategy = ResizeStrategy::All(rect.width().to_u32().unwrap_or_default(), rect.height().to_u32().unwrap_or_default());

        gl.resize(strategy);

        self.camera
            .borrow_mut()
            .as_mut()
            .unwrap()
            .update_viewport(&mut gl);
    }
}

fn compile_shaders(gl: &mut WebGl2Renderer) -> Result<GraphShaders> {
    let quad_unit_vert = {
        let mut s = SHADER_VERTEX_QUAD_UNIT
            .replace("%% INCLUDE_COMMON_MATH %%", SHADER_COMMON_MATH)
            .replace("%% INCLUDE_COMMON_CAMERA %%", SHADER_COMMON_CAMERA);

        gl.compile_shader(&s, ShaderType::Vertex)?
    };

    let sphere_unit_vert = {
        let mut s = SHADER_VERTEX_SPHERE_UNIT
            .replace("%% INCLUDE_COMMON_MATH %%", SHADER_COMMON_MATH)
            .replace("%% INCLUDE_COMMON_CAMERA %%", SHADER_COMMON_CAMERA);

        gl.compile_shader(&s, ShaderType::Vertex)?
    };

    let unlit_diffuse_frag = {
        let mut s = SHADER_FRAGMENT_UNLIT_DIFFUSE
            .replace("%% INCLUDE_COMMON_MATH %%", SHADER_COMMON_MATH)
            .replace("%% INCLUDE_COMMON_CAMERA %%", SHADER_COMMON_CAMERA);

        gl.compile_shader(&s, ShaderType::Fragment)?
    };

    let mesh_frag = {
        let mut s = SHADER_FRAGMENT_MESH
            .replace("%% INCLUDE_COMMON_MATH %%", SHADER_COMMON_MATH)
            .replace("%% INCLUDE_COMMON_CAMERA %%", SHADER_COMMON_CAMERA);

        gl.compile_shader(&s, ShaderType::Fragment)?
    };

    let quad_program = gl.compile_program(&vec![quad_unit_vert, unlit_diffuse_frag])?;
    let sphere_program = gl.compile_program(&vec![sphere_unit_vert, mesh_frag])?;

    for program_id in [sphere_program, quad_program] {
        gl.init_uniform_buffer_name(program_id, "ubo_camera")?;
        //gl.init_uniform_buffer_name(program_id, "ubo_lights")?;
    }

    Ok(GraphShaders {
        sphere_program,
        quad_program,
    })
}

pub fn create_geometry(gl: &mut WebGl2Renderer) -> Result<GraphGeometry> {
    let mut sphere_geom = super::icosahedron::Polyhedron::new_truncated_isocahedron(1.0, 3);

    sphere_geom.assign_random_face_colors();
    sphere_geom.compute_triangle_normals();

    // positions
    let mut positions: Vec<f32> = Vec::with_capacity(sphere_geom.positions.len() * 3);
    for pos in sphere_geom.positions.iter() {
        positions.push(pos.0.x);
        positions.push(pos.0.y);
        positions.push(pos.0.z);
    }
    let sphere_pos_id = gl.create_buffer()?;
    gl.upload_buffer(
        sphere_pos_id,
        BufferData::new(
            &positions,
            BufferTarget::ArrayBuffer,
            BufferUsage::StaticDraw,
        ),
    )?;

    // normals
    let mut normals: Vec<f32> = Vec::with_capacity(sphere_geom.normals.len() * 3);
    for normal in sphere_geom.normals.iter() {
        normals.push(normal.0.x);
        normals.push(normal.0.y);
        normals.push(normal.0.z);
    }

    let sphere_normal_id = gl.create_buffer()?;
    gl.upload_buffer(
        sphere_normal_id,
        BufferData::new(&normals, BufferTarget::ArrayBuffer, BufferUsage::StaticDraw),
    )?;

    // colors
    let mut colors: Vec<f32> = Vec::with_capacity(sphere_geom.colors.len() * 3);
    for color in sphere_geom.colors.iter() {
        colors.push(color.0.x);
        colors.push(color.0.y);
        colors.push(color.0.z);
    }
    let sphere_color_id = gl.create_buffer()?;
    gl.upload_buffer(
        sphere_color_id,
        BufferData::new(&colors, BufferTarget::ArrayBuffer, BufferUsage::StaticDraw),
    )?;

    // indices
    let mut indices: Vec<u32> = Vec::with_capacity(sphere_geom.cells.len() * 3);
    for cell in sphere_geom.cells.iter() {
        indices.push(cell.a.to_u32().unwrap_or_default());
        indices.push(cell.b.to_u32().unwrap_or_default());
        indices.push(cell.c.to_u32().unwrap_or_default());
    }
    let sphere_indices_id = gl.create_buffer()?;

    gl.upload_buffer(
        sphere_indices_id,
        BufferData::new(
            &indices,
            BufferTarget::ElementArrayBuffer,
            BufferUsage::StaticDraw,
        ),
    )?;

    Ok(GraphGeometry {
        sphere_pos_id,
        sphere_normal_id,
        sphere_indices_id,
        sphere_color_id,
        sphere_index_count: indices.len().to_u32().unwrap_or_default(),
    })
}

impl GraphMesh {
    pub fn update_transform(&mut self) {
        let mut transform = Mat4::IDENTITY;
        self._transform_matrix = transform
            * Mat4::from_scale_rotation_translation(self.scale, self.rotation, self.position);
    }
    pub fn animate(&mut self, time: f32) {
        let speed = 0.001;
        self.rotation = self.rotation * Quat::from_rotation_y(time * speed);
        self.rotation = self.rotation * Quat::from_rotation_x(time * speed);
        self.rotation = self.rotation * Quat::from_rotation_z(time * speed);

        self.update_transform();
        // let mut transform = self.get_transform();
        // transform.set_rotation_y(time);
        // transform.set_rotation_x(time);
    }
}
