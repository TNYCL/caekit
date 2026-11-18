//! Conforming tetrahedral meshes and geometric operators in global coordinates.

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use crate::{MeshMetrics, MeshQuality};
use caekit_types::Error;
use caekit_types::error::nonempty;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TetraMesh {
    pub vertices_m: Vec<[f64; 3]>,
    /// Positive orientation, with the three edge vectors measured from the first vertex.
    pub cells: Vec<[usize; 4]>,
    /// Named sets of exterior triangles. Each exterior face belongs to one region.
    pub boundary_regions: Vec<FaceRegion>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceRegion {
    pub name: String,
    pub faces: Vec<[usize; 3]>,
}

pub fn subtract(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    std::array::from_fn(|i| a[i] - b[i])
}

pub fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

pub fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}

#[derive(Debug)]
pub struct Tet {
    pub volume: f64,
    pub gradients: [[f64; 3]; 4],
    pub centroid: [f64; 3],
}

#[derive(Debug)]
pub struct Triangle {
    pub nodes: [usize; 3],
    pub area: f64,
    pub outward: [f64; 3],
}

pub struct TetraTopology<'a> {
    pub elements: Vec<Tet>,
    pub boundaries: BTreeMap<&'a str, Vec<Triangle>>,
    pub metrics: MeshMetrics,
}

/// A checked topology bound to the mesh whose node and cell ordering it follows.
pub struct CheckedTetraMesh<'a> {
    mesh: &'a TetraMesh,
    topology: TetraTopology<'a>,
}

impl<'a> CheckedTetraMesh<'a> {
    pub fn mesh(&self) -> &'a TetraMesh {
        self.mesh
    }

    pub fn topology(&self) -> &TetraTopology<'a> {
        &self.topology
    }

    pub fn into_metrics(self) -> MeshMetrics {
        self.topology.metrics
    }
}

#[derive(Debug)]
struct FaceUse {
    first: usize,
    normal_points_in: bool,
    other: Option<usize>,
    boundary: Triangle,
}

impl TetraMesh {
    pub fn checked<'a>(&'a self, path: &str) -> Result<CheckedTetraMesh<'a>, Error> {
        Ok(CheckedTetraMesh {
            mesh: self,
            topology: self.check(path)?,
        })
    }

    pub fn check<'a>(&'a self, path: &str) -> Result<TetraTopology<'a>, Error> {
        let vertices = &self.vertices_m;
        if vertices.len() < 4 || self.cells.is_empty() {
            return Err(Error::invalid(
                path,
                "requires at least four vertices and one tetrahedron",
            ));
        }
        for (i, point) in vertices.iter().enumerate() {
            if !point.iter().all(|x| x.is_finite()) {
                return Err(Error::invalid(
                    format!("{path}.vertices_m[{i}]"),
                    "coordinates must be finite",
                ));
            }
        }

        let mut elements = Vec::with_capacity(self.cells.len());
        let mut faces: BTreeMap<[usize; 3], FaceUse> = BTreeMap::new();
        let mut cell_ids = BTreeSet::new();
        let mut used = vec![false; vertices.len()];
        let mut parent: Vec<_> = (0..self.cells.len()).collect();
        let mut metrics = MeshMetrics {
            name: path.to_owned(),
            cell_count: self.cells.len(),
            smallest_cell_m: f64::INFINITY,
            smallest_cell_index: 0,
            largest_cell_m: 0.0,
            quality: Some(MeshQuality {
                minimum_mean_ratio: f64::INFINITY,
                cell_index: 0,
            }),
        };

        for (i, &nodes) in self.cells.iter().enumerate() {
            let mut ids = nodes;
            ids.sort_unstable();
            if ids[3] >= vertices.len() || ids.windows(2).any(|pair| pair[0] == pair[1]) {
                return Err(cell_error(
                    path,
                    i,
                    "tetrahedron requires four distinct in-range vertices",
                ));
            }
            if !cell_ids.insert(ids) {
                return Err(cell_error(path, i, "duplicate tetrahedron"));
            }
            for node in nodes {
                used[node] = true;
            }
            let points = nodes.map(|node| vertices[node]);
            let (element, shortest, longest, quality) = element(points).ok_or_else(|| {
                cell_error(
                    path,
                    i,
                    "tetrahedron is inverted, degenerate, or outside the finite geometric range",
                )
            })?;
            elements.push(element);
            if shortest < metrics.smallest_cell_m {
                metrics.smallest_cell_m = shortest;
                metrics.smallest_cell_index = i;
            }
            metrics.largest_cell_m = metrics.largest_cell_m.max(longest);
            let worst = metrics.quality.as_mut().expect("tetrahedral quality");
            if quality < worst.minimum_mean_ratio {
                worst.minimum_mean_ratio = quality;
                worst.cell_index = i;
            }

            for (opposite, &opposite_point) in points.iter().enumerate() {
                let mut face = [0; 3];
                for (target, node) in face.iter_mut().zip(
                    nodes
                        .iter()
                        .enumerate()
                        .filter_map(|(j, &node)| (j != opposite).then_some(node)),
                ) {
                    *target = node;
                }
                face.sort_unstable();
                let [p, q, r] = face.map(|node| vertices[node]);
                let (area, normal, normal_points_in) =
                    face_geometry(p, q, r, opposite_point, longest).ok_or_else(|| {
                        cell_error(
                            path,
                            i,
                            "face area or orientation is numerically degenerate",
                        )
                    })?;

                match faces.entry(face) {
                    std::collections::btree_map::Entry::Vacant(entry) => {
                        entry.insert(FaceUse {
                            first: i,
                            normal_points_in,
                            other: None,
                            boundary: Triangle {
                                nodes: face,
                                area,
                                outward: normal,
                            },
                        });
                    }
                    std::collections::btree_map::Entry::Occupied(mut entry) => {
                        let owner = entry.get_mut();
                        if owner.other.is_some() {
                            return Err(cell_error(
                                path,
                                i,
                                "a triangle is shared by more than two tetrahedra",
                            ));
                        }
                        if owner.normal_points_in == normal_points_in {
                            return Err(cell_error(
                                path,
                                i,
                                "adjacent tetrahedra lie on the same side of their shared face",
                            ));
                        }
                        owner.other = Some(i);
                        let a = root(&mut parent, owner.first);
                        let b = root(&mut parent, i);
                        parent[b] = a;
                    }
                }
            }
        }

        if let Some(unused) = used.iter().position(|assigned| !assigned) {
            return Err(Error::invalid(
                format!("{path}.vertices_m[{unused}]"),
                "vertex is unused",
            ));
        }
        let first = root(&mut parent, 0);
        if let Some(disconnected) = (1..parent.len()).find(|&i| root(&mut parent, i) != first) {
            return Err(cell_error(
                path,
                disconnected,
                "the solid mesh must be connected through faces",
            ));
        }

        let mut boundaries = BTreeMap::new();
        for (i, region) in self.boundary_regions.iter().enumerate() {
            let assigned = &region.faces;
            let region = &region.name;
            let region_path = format!("{path}.boundary_regions[{i}]");
            nonempty(&region_path, region)?;
            if boundaries.contains_key(region.as_str()) {
                return Err(Error::invalid(region_path, "duplicate region name"));
            }
            if assigned.is_empty() {
                return Err(Error::invalid(region_path, "region has no faces"));
            }
            let mut triangles = Vec::with_capacity(assigned.len());
            for (i, &face) in assigned.iter().enumerate() {
                let mut key = face;
                key.sort_unstable();
                let Some(owner) = faces.remove(&key).filter(|owner| owner.other.is_none()) else {
                    return Err(Error::invalid(
                        format!("{region_path}[{i}]"),
                        "region references an unknown, interior, or already assigned face",
                    ));
                };
                triangles.push(owner.boundary);
            }
            boundaries.insert(region.as_str(), triangles);
        }

        if let Some((nodes, _)) = faces.iter().find(|(_, owner)| owner.other.is_none()) {
            return Err(Error::invalid(
                format!("{path}.boundary_regions"),
                format!("exterior face {nodes:?} has no region assignment"),
            ));
        }
        Ok(TetraTopology {
            elements,
            boundaries,
            metrics,
        })
    }
}

fn root(parent: &mut [usize], mut i: usize) -> usize {
    while parent[i] != i {
        parent[i] = parent[parent[i]];
        i = parent[i];
    }
    i
}

fn cell_error(path: &str, cell: usize, message: &str) -> Error {
    Error::invalid(format!("{path}.cells[{cell}]"), message)
}

fn element(p: [[f64; 3]; 4]) -> Option<(Tet, f64, f64, f64)> {
    let edges = [
        subtract(p[1], p[0]),
        subtract(p[2], p[0]),
        subtract(p[3], p[0]),
        subtract(p[2], p[1]),
        subtract(p[3], p[1]),
        subtract(p[3], p[2]),
    ];
    let lengths = edges.map(length);
    let shortest = lengths.into_iter().fold(f64::INFINITY, f64::min);
    let longest = lengths.into_iter().fold(0.0, f64::max);
    if shortest <= 0.0 || !longest.is_finite() {
        return None;
    }

    let [a, b, c, ..] = edges.map(|v| v.map(|x| x / longest));
    let determinant = dot(a, cross(b, c));
    if determinant <= 64.0 * f64::EPSILON || !determinant.is_finite() {
        return None;
    }
    let volume = ((determinant / 6.0) * longest * longest) * longest;
    if !volume.is_finite() || volume <= 0.0 {
        return None;
    }
    let [g1, g2, g3] =
        [cross(b, c), cross(c, a), cross(a, b)].map(|v| v.map(|x| (x / determinant) / longest));
    let g0 = std::array::from_fn(|i| -(g1[i] + g2[i] + g3[i]));
    let gradients = [g0, g1, g2, g3];
    if !gradients.iter().flatten().all(|x| x.is_finite()) {
        return None;
    }
    let squared_edges: f64 = lengths
        .into_iter()
        .map(|length| (length / longest).powi(2))
        .sum();
    let quality = (12.0 * (determinant / 2.0).powf(2.0 / 3.0) / squared_edges).min(1.0);
    let centroid = std::array::from_fn(|i| p.iter().map(|point| point[i] * 0.25).sum());
    Some((
        Tet {
            volume,
            gradients,
            centroid,
        },
        shortest,
        longest,
        quality,
    ))
}

fn face_geometry(
    p: [f64; 3],
    q: [f64; 3],
    r: [f64; 3],
    opposite: [f64; 3],
    scale: f64,
) -> Option<(f64, [f64; 3], bool)> {
    let local = |a| subtract(a, p).map(|x| x / scale);
    let normal = cross(local(q), local(r));
    let magnitude = length(normal);
    let area = magnitude * 0.5 * scale * scale;
    let direction = dot(normal, local(opposite));
    if !area.is_finite() || area <= 0.0 || direction == 0.0 {
        return None;
    }
    let normal_points_in = direction > 0.0;
    let sign = if normal_points_in { -1.0 } else { 1.0 };
    let outward = normal.map(|x| sign * x / magnitude);
    Some((area, outward, normal_points_in))
}

fn length(v: [f64; 3]) -> f64 {
    v.into_iter().fold(0.0_f64, f64::hypot)
}
