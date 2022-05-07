use crate::config::ClothConfig;
use crate::stick::Stick;
use bevy::ecs::component::Component;
use bevy::log;
use bevy::math::{Mat4, Vec3};
use bevy::render::mesh::{Indices, Mesh, VertexAttributeValues};
use bevy::utils::HashSet;

macro_rules! get_point {
    ($id:expr, $points:expr, $fixed_points:expr, $matrix:expr) => {
        match $points.get($id) {
            None => {
                log::warn!("Failed to retrieve a Cloth point at index {}", $id);
                continue;
            }
            Some(p) => {
                if $fixed_points.contains(&$id) {
                    ($matrix.transform_point3(*p), true)
                } else {
                    (*p, false)
                }
            }
        }
    };
}

/// Cloth component
#[derive(Debug, Clone, Component, Default)]
#[must_use]
pub struct Cloth {
    /// cloth points unaffected by physics and following the attached `GlobalTransform`.
    pub fixed_points: HashSet<usize>,
    /// Current Cloth points 3D positions in world space
    current_point_positions: Vec<Vec3>,
    /// Old Cloth points 3D positions in world space
    previous_point_positions: Vec<Vec3>,
    /// Cloth sticks linking points
    sticks: Vec<Stick>,
}

impl Cloth {
    #[inline]
    pub fn new(fixed_points: impl Iterator<Item = usize>) -> Self {
        Self {
            fixed_points: fixed_points.collect(),
            current_point_positions: vec![],
            previous_point_positions: vec![],
            sticks: vec![],
        }
    }

    #[inline]
    #[must_use]
    pub fn is_setup(&self) -> bool {
        !self.current_point_positions.is_empty()
    }

    pub fn apply_to_mesh(&self, mesh: &mut Mesh, transform_matrix: &Mat4) {
        let matrix = transform_matrix.inverse();

        let positions: Vec<[f32; 3]> = self
            .current_point_positions
            .iter()
            .enumerate()
            .map(|(i, p)| {
                if self.fixed_points.contains(&i) {
                    p.to_array()
                } else {
                    matrix.transform_point3(*p).to_array()
                }
            })
            .collect();
        mesh.insert_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    }

    pub fn init_from_mesh(&mut self, mesh: &Mesh, transform_matrix: &Mat4) {
        let vertex_positions = mesh
            .attribute(Mesh::ATTRIBUTE_POSITION)
            .expect("Mesh associated to cloth doesn't have `ATTRIBUTE_POSITION` set");
        let positions: Vec<Vec3> = match vertex_positions {
            VertexAttributeValues::Float32x3(v) => v
                .iter()
                .map(|p| transform_matrix.transform_point3(Vec3::from(*p)))
                .collect(),
            _ => {
                panic!("Unsupported vertex position attribute, only `Float32x3` is supported");
            }
        };
        let indices: Vec<usize> = match mesh.indices() {
            None => {
                log::error!("Mesh associated to cloth doesn't have indices set");
                return;
            }
            Some(i) => match i {
                Indices::U16(v) => v.iter().map(|i| *i as usize).collect(),
                Indices::U32(v) => v.iter().map(|i| *i as usize).collect(),
            },
        };
        let sticks = indices
            .chunks_exact(3)
            .flat_map(|truple| {
                let [a, b, c] = [truple[0], truple[1], truple[2]];
                let (p_a, p_b, p_c) = (positions[a], positions[b], positions[c]);
                vec![
                    Stick {
                        point_a_index: a,
                        point_b_index: b,
                        length: p_a.distance(p_b),
                    },
                    Stick {
                        point_a_index: b,
                        point_b_index: c,
                        length: p_b.distance(p_c),
                    },
                    Stick {
                        point_a_index: c,
                        point_b_index: a,
                        length: p_c.distance(p_a),
                    },
                ]
            })
            .collect();
        self.sticks = sticks;
        self.previous_point_positions = positions.clone();
        self.current_point_positions = positions;
    }

    pub fn update(&mut self, config: &ClothConfig, delta_time: f32, transform_matrix: &Mat4) {
        self.update_points(delta_time, config);
        self.update_sticks(config, transform_matrix);
    }

    fn update_points(&mut self, delta_time: f32, config: &ClothConfig) {
        let gravity = config.gravity * delta_time * delta_time;
        let friction = config.friction_coefficient();

        for (i, point) in self.current_point_positions.iter_mut().enumerate() {
            if !self.fixed_points.contains(&i) {
                let velocity = *point - self.previous_point_positions[i];
                self.previous_point_positions[i] = *point;
                *point += velocity * friction * delta_time + gravity;
            }
        }
    }

    fn update_sticks(&mut self, config: &ClothConfig, matrix: &Mat4) {
        for _depth in 0..config.sticks_computation_depth {
            for stick in &self.sticks {
                let (position_a, fixed_a) = get_point!(
                    stick.point_a_index,
                    self.current_point_positions,
                    self.fixed_points,
                    matrix
                );
                let (position_b, fixed_b) = get_point!(
                    stick.point_b_index,
                    self.current_point_positions,
                    self.fixed_points,
                    matrix
                );
                let target_len = if fixed_a == fixed_b {
                    stick.length / 2.0
                } else {
                    stick.length
                };
                let center = (position_b + position_a) / 2.0;
                let direction = match (position_b - position_a).try_normalize() {
                    None => {
                        log::warn!("Failed handle stick between points {} and {} which are too close to each other", stick.point_a_index, stick.point_b_index);
                        continue;
                    }
                    Some(dir) => dir * target_len,
                };
                if !fixed_a {
                    self.current_point_positions[stick.point_a_index] = center + direction;
                }
                if !fixed_b {
                    self.current_point_positions[stick.point_b_index] = center - direction;
                }
            }
        }
    }
}
