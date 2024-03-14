use crate::prelude::*;
use awsm_web::webgl::{BufferUsage, Id, WebGl2Renderer};
use glam::{Mat4, Vec3};
use num_traits::ToPrimitive;

pub struct Camera {
    pub buffer_id: Id,
    pub scratch_buffer: [f32; 36],
    pub view: Mat4,
    pub projection: Mat4,
    pub eye: Vec3,
}

const UBO_CAMERA: u32 = 0;

impl Camera {
    pub fn new(gl: &mut WebGl2Renderer) -> Result<Self> {
        gl.hardcoded_ubo_locations
            .insert("ubo_camera".to_string(), UBO_CAMERA);

        let buffer_id = gl.create_buffer()?;

        let eye = Vec3::new(0.0, 0.0, -5.0);
        let view = Mat4::look_at_lh(eye, Vec3::new(0.0, 0.0, 0.0), Vec3::new(0.0, 1.0, 0.0));

        let projection = Mat4::IDENTITY;

        let mut _self = Self {
            buffer_id,
            scratch_buffer: [0.0; 36],
            view,
            projection,
            eye,
        };

        _self.update_viewport(gl)?;

        Ok(_self)
    }

    pub fn update_viewport(&mut self, gl: &WebGl2Renderer) -> Result<()> {
        let (_, _, width, height) = gl.get_viewport();
        let aspect = width.to_f32().unwrap_or_default() / height.to_f32().unwrap_or(1.0);
        self.projection = Mat4::perspective_lh(std::f32::consts::PI / 4.0, aspect, 0.1, 10000.0);
        Ok(())
    }

    pub fn update_ubo(&mut self, gl: &mut WebGl2Renderer) -> Result<()> {
        self.view
            .write_cols_to_slice(&mut self.scratch_buffer[0..16]);
        self.projection
            .write_cols_to_slice(&mut self.scratch_buffer[16..32]);
        self.eye.write_to_slice(&mut self.scratch_buffer[32..]);
        gl.upload_uniform_buffer_f32(
            self.buffer_id,
            &self.scratch_buffer,
            BufferUsage::DynamicDraw,
        )?;

        gl.activate_uniform_buffer_loc(self.buffer_id, UBO_CAMERA);

        Ok(())
    }
}
