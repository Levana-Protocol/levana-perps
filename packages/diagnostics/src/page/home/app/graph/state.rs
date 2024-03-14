use std::collections::VecDeque;

use awsm_web::{
    dom::resize::ResizeObserver,
    tick::{MainLoop, Raf},
    webgl::{AttributeOptions, DataType, Id, NameOrLoc, VertexArray, WebGl2Renderer},
};
use glam::{Mat4, Quat, Vec3};
use msg::{contracts::market::config::Config as MarketConfig, token::Token};

use crate::{
    page::home::app::{controls::Controls, stats::Stats},
    prelude::*,
};

use super::camera::Camera;

pub struct Graph {
    pub bridge: Rc<Bridge>,
    pub market_id: MarketId,
    pub market_type: MarketType,
    pub market_token: Token,
    pub market_config: MarketConfig,
    pub stats: Rc<Stats>,
    pub controls: Rc<Controls>,
    pub raf: RefCell<Option<Raf>>,
    pub gl: RefCell<Option<WebGl2Renderer>>,
    pub resize_observer: RefCell<Option<ResizeObserver>>,
    pub shaders: RefCell<Option<GraphShaders>>,
    pub geometry: RefCell<Option<GraphGeometry>>,
    pub meshes: RefCell<Vec<GraphMesh>>,
    pub camera: RefCell<Option<Camera>>,
}

pub struct GraphShaders {
    pub sphere_program: Id,
    pub quad_program: Id,
}

pub struct GraphGeometry {
    pub sphere_pos_id: Id,
    pub sphere_normal_id: Id,
    pub sphere_color_id: Id,
    pub sphere_indices_id: Id,
    pub sphere_index_count: u32,
}

pub struct GraphMesh {
    pub vao_id: Id,
    pub program_id: Id,
    pub position: Vec3,
    pub rotation: Quat,
    pub scale: Vec3,
    pub index_count: u32,
    pub(super) _transform_matrix: Mat4,
}

impl Graph {
    pub fn new(
        bridge: Rc<Bridge>,
        market_id: MarketId,
        market_type: MarketType,
        market_token: Token,
        market_config: MarketConfig,
        stats: Rc<Stats>,
        controls: Rc<Controls>,
    ) -> Rc<Self> {
        let _self = Rc::new(Self {
            bridge,
            market_id,
            market_type,
            market_token,
            market_config,
            stats,
            controls,
            raf: RefCell::new(None),
            gl: RefCell::new(None),
            resize_observer: RefCell::new(None),
            shaders: RefCell::new(None),
            geometry: RefCell::new(None),
            meshes: RefCell::new(Vec::new()),
            camera: RefCell::new(None),
        });

        _self
    }

    pub fn new_mesh(&self) -> Result<GraphMesh> {
        let mut gl = self.gl.borrow_mut();
        let gl = gl.as_mut().unwrap();
        let geometry = self.geometry.borrow();
        let geometry = geometry.as_ref().unwrap();

        let program_id = self.shaders.borrow().as_ref().unwrap().sphere_program;

        gl.activate_program(program_id)?;
        let vao_id = gl.create_vertex_array()?;

        gl.assign_vertex_array(
            vao_id,
            Some(geometry.sphere_indices_id),
            &[
                VertexArray {
                    attribute: NameOrLoc::Name("a_vertex"),
                    buffer_id: geometry.sphere_pos_id,
                    opts: AttributeOptions {
                        size: 3,
                        data_type: DataType::Float,
                        normalized: false,
                        stride: 0_u8,
                        offset: 0_u64,
                        is_int_array: false, // ??
                    },
                },
                VertexArray {
                    attribute: NameOrLoc::Name("a_normal"),
                    buffer_id: geometry.sphere_normal_id,
                    opts: AttributeOptions {
                        size: 3,
                        data_type: DataType::Float,
                        normalized: false,
                        stride: 0_u8,
                        offset: 0_u64,
                        is_int_array: false, // ??
                    },
                },
                VertexArray {
                    attribute: NameOrLoc::Name("a_color"),
                    buffer_id: geometry.sphere_color_id,
                    opts: AttributeOptions {
                        size: 3,
                        data_type: DataType::Float,
                        normalized: false,
                        stride: 0_u8,
                        offset: 0_u64,
                        is_int_array: false, // ??
                    },
                },
            ],
        )?;

        let position = Vec3::ZERO;
        let scale = Vec3::ONE;
        let rotation = Quat::IDENTITY;

        Ok(GraphMesh {
            vao_id,
            program_id,
            index_count: geometry.sphere_index_count,
            position,
            rotation,
            scale,
            _transform_matrix: Mat4::IDENTITY,
        })
    }
}

impl GraphMesh {
    pub fn get_transform(&self) -> &Mat4 {
        &self._transform_matrix
    }
}
