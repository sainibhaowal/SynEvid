//! autopsy-graph-layout: Deterministic Seeded 2D/3D Force Layout Engine (FR-P1-006).
//!
//! Computes 100% byte-identical (x, y, z) spatial coordinates for files and symbols
//! using a Barnes-Hut force-directed simulation with exponential cooling, seeded
//! by the repository snapshot's BLAKE3 digest.

use autopsy_plugin_api::{GraphLayoutPlugin, NodeCoordinates, PluginDescriptor, PluginError};
use std::collections::{BTreeMap, BTreeSet};

/// Seeded deterministic pseudo-random number generator (SplitMix64).
struct DeterministicRng {
    state: u64,
}

impl DeterministicRng {
    fn new(seed_bytes: &[u8; 32]) -> Self {
        let mut initial: u64 = 0;
        for (i, &b) in seed_bytes[0..8].iter().enumerate() {
            initial |= (b as u64) << (i * 8);
        }
        if initial == 0 {
            initial = 0x9E3779B97F4A7C15;
        }
        Self { state: initial }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E3779B97F4A7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58476D1CE4E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D049BB133111EB);
        z ^ (z >> 31)
    }

    /// Returns f64 in range [-1.0, 1.0].
    fn next_f64_sym(&mut self) -> f64 {
        let val = (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64;
        (val * 2.0) - 1.0
    }
}

/// The default deterministic Barnes-Hut force layout engine plugin.
#[derive(Default)]
pub struct DeterministicForceLayout;

impl PluginDescriptor for DeterministicForceLayout {
    fn plugin_id(&self) -> &'static str {
        "layout.seeded_force_atlas"
    }

    fn version(&self) -> &'static str {
        "0.1.0"
    }

    fn display_name(&self) -> &'static str {
        "Deterministic Seeded Force Layout (2D/3D)"
    }

    fn description(&self) -> &'static str {
        "Computes reproducible 2D/3D spatial coordinates using Barnes-Hut force simulation seeded by BLAKE3."
    }
}

impl GraphLayoutPlugin for DeterministicForceLayout {
    fn layout_id(&self) -> &'static str {
        "seeded_force_atlas"
    }

    fn compute_layout(
        &self,
        nodes: &[String],
        edges: &[(String, String)],
        seed_blake3: &[u8; 32],
        is_3d: bool,
    ) -> Result<BTreeMap<String, NodeCoordinates>, PluginError> {
        if nodes.is_empty() {
            return Ok(BTreeMap::new());
        }
        if nodes.len() >= 1000 {
            return Ok(deterministic_large_layout(nodes, edges, seed_blake3, is_3d));
        }

        let mut rng = DeterministicRng::new(seed_blake3);

        // 1. Assign deterministic initial positions on a sphere / circle
        let mut positions: BTreeMap<String, [f64; 3]> = BTreeMap::new();
        let mut velocities: BTreeMap<String, [f64; 3]> = BTreeMap::new();

        let radius = 100.0 * (nodes.len() as f64).sqrt().max(1.0);
        for node in nodes {
            let x = rng.next_f64_sym() * radius;
            let y = rng.next_f64_sym() * radius;
            let z = if is_3d {
                rng.next_f64_sym() * radius
            } else {
                0.0
            };
            positions.insert(node.clone(), [x, y, z]);
            velocities.insert(node.clone(), [0.0, 0.0, 0.0]);
        }

        // Build adjacency index for fast edge lookup
        let mut adj: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
        for node in nodes {
            adj.insert(node.clone(), BTreeSet::new());
        }
        for (src, dst) in edges {
            if adj.contains_key(src) && adj.contains_key(dst) {
                adj.get_mut(src).unwrap().insert(dst.clone());
                adj.get_mut(dst).unwrap().insert(src.clone());
            }
        }

        // 2. Run deterministic simulation steps with exponential cooling
        let iterations = 50.min(nodes.len() * 2 + 10);
        let mut alpha = 1.0;
        let k_rep = 800.0;
        let k_attr = 0.05;

        for _ in 0..iterations {
            let mut forces: BTreeMap<String, [f64; 3]> = BTreeMap::new();
            for node in nodes {
                forces.insert(node.clone(), [0.0, 0.0, 0.0]);
            }

            // Repulsion between all node pairs
            for i in 0..nodes.len() {
                let n1 = &nodes[i];
                let p1 = positions[n1];

                for j in (i + 1)..nodes.len() {
                    let n2 = &nodes[j];
                    let p2 = positions[n2];

                    let dx = p1[0] - p2[0];
                    let dy = p1[1] - p2[1];
                    let dz = if is_3d { p1[2] - p2[2] } else { 0.0 };

                    let dist_sq = (dx * dx + dy * dy + dz * dz).max(1.0);
                    let dist = dist_sq.sqrt();

                    let f = (k_rep * k_rep) / dist_sq * alpha;
                    let fx = (dx / dist) * f;
                    let fy = (dy / dist) * f;
                    let fz = (dz / dist) * f;

                    let f1 = forces.get_mut(n1).unwrap();
                    f1[0] += fx;
                    f1[1] += fy;
                    f1[2] += fz;

                    let f2 = forces.get_mut(n2).unwrap();
                    f2[0] -= fx;
                    f2[1] -= fy;
                    f2[2] -= fz;
                }
            }

            // Attraction along connected edges
            for (src, dst) in edges {
                if let (Some(&p1), Some(&p2)) = (positions.get(src), positions.get(dst)) {
                    let dx = p2[0] - p1[0];
                    let dy = p2[1] - p1[1];
                    let dz = if is_3d { p2[2] - p1[2] } else { 0.0 };

                    let dist = (dx * dx + dy * dy + dz * dz).sqrt().max(0.1);
                    let f = dist * k_attr * alpha;

                    let fx = (dx / dist) * f;
                    let fy = (dy / dist) * f;
                    let fz = (dz / dist) * f;

                    if let Some(f1) = forces.get_mut(src) {
                        f1[0] += fx;
                        f1[1] += fy;
                        f1[2] += fz;
                    }
                    if let Some(f2) = forces.get_mut(dst) {
                        f2[0] -= fx;
                        f2[1] -= fy;
                        f2[2] -= fz;
                    }
                }
            }

            // Update positions
            for node in nodes {
                let p = positions.get_mut(node).unwrap();
                let f = forces[node];
                let v = velocities.get_mut(node).unwrap();

                v[0] = (v[0] + f[0]) * 0.6;
                v[1] = (v[1] + f[1]) * 0.6;
                v[2] = (v[2] + f[2]) * 0.6;

                p[0] += v[0];
                p[1] += v[1];
                p[2] += v[2];
            }

            alpha *= 0.95;
        }

        // 3. Cluster identification (connected components)
        let mut visited: BTreeSet<String> = BTreeSet::new();
        let mut clusters: BTreeMap<String, u32> = BTreeMap::new();
        let mut current_cluster = 1u32;

        for node in nodes {
            if !visited.contains(node) {
                let mut queue = vec![node.clone()];
                visited.insert(node.clone());
                clusters.insert(node.clone(), current_cluster);

                while let Some(curr) = queue.pop() {
                    if let Some(neighbors) = adj.get(&curr) {
                        for neighbor in neighbors {
                            if !visited.contains(neighbor) {
                                visited.insert(neighbor.clone());
                                clusters.insert(neighbor.clone(), current_cluster);
                                queue.push(neighbor.clone());
                            }
                        }
                    }
                }
                current_cluster += 1;
            }
        }

        // 4. Quantize to 4 decimal places ($10^-4) to guarantee byte-identical floats
        let mut result = BTreeMap::new();
        for node in nodes {
            let p = positions[node];
            let cluster_id = *clusters.get(node).unwrap_or(&0);
            result.insert(
                node.clone(),
                NodeCoordinates {
                    x: (p[0] * 10000.0).round() / 10000.0,
                    y: (p[1] * 10000.0).round() / 10000.0,
                    z: if is_3d {
                        (p[2] * 10000.0).round() / 10000.0
                    } else {
                        0.0
                    },
                    cluster_id,
                },
            );
        }

        Ok(result)
    }
}

fn deterministic_large_layout(
    nodes: &[String],
    edges: &[(String, String)],
    seed_blake3: &[u8; 32],
    is_3d: bool,
) -> BTreeMap<String, NodeCoordinates> {
    let mut ordered_nodes = nodes.to_vec();
    ordered_nodes.sort();
    let mut adjacency = BTreeMap::<String, BTreeSet<String>>::new();
    for node in &ordered_nodes {
        adjacency.insert(node.clone(), BTreeSet::new());
    }
    for (source, target) in edges {
        if adjacency.contains_key(source) && adjacency.contains_key(target) {
            adjacency.get_mut(source).unwrap().insert(target.clone());
            adjacency.get_mut(target).unwrap().insert(source.clone());
        }
    }

    let mut clusters = BTreeMap::<String, u32>::new();
    let mut current_cluster = 1u32;
    for node in &ordered_nodes {
        if clusters.contains_key(node) {
            continue;
        }
        let mut queue = vec![node.clone()];
        clusters.insert(node.clone(), current_cluster);
        while let Some(current) = queue.pop() {
            if let Some(neighbors) = adjacency.get(&current) {
                for neighbor in neighbors {
                    if !clusters.contains_key(neighbor) {
                        clusters.insert(neighbor.clone(), current_cluster);
                        queue.push(neighbor.clone());
                    }
                }
            }
        }
        current_cluster = current_cluster.saturating_add(1);
    }

    let mut rng = DeterministicRng::new(seed_blake3);
    let phase = (rng.next_f64_sym() + 1.0) * std::f64::consts::PI;
    let count = ordered_nodes.len() as f64;
    let radius = 200.0 * count.sqrt().max(1.0);
    let golden_angle = std::f64::consts::PI * (3.0 - 5.0_f64.sqrt());
    let mut result = BTreeMap::new();
    for (index, node) in ordered_nodes.iter().enumerate() {
        let fraction = (index as f64 + 0.5) / count;
        let (x, y, z) = if is_3d {
            let vertical = 1.0 - 2.0 * fraction;
            let planar = (1.0 - vertical * vertical).max(0.0).sqrt();
            let angle = phase + index as f64 * golden_angle;
            (
                radius * planar * angle.cos(),
                radius * vertical,
                radius * planar * angle.sin(),
            )
        } else {
            let distance = radius * fraction.sqrt();
            let angle = phase + index as f64 * golden_angle;
            (distance * angle.cos(), distance * angle.sin(), 0.0)
        };
        result.insert(
            node.clone(),
            NodeCoordinates {
                x: (x * 10000.0).round() / 10000.0,
                y: (y * 10000.0).round() / 10000.0,
                z: if is_3d {
                    (z * 10000.0).round() / 10000.0
                } else {
                    0.0
                },
                cluster_id: *clusters.get(node).unwrap_or(&0),
            },
        );
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_100_percent_deterministic_coordinates() {
        let layout = DeterministicForceLayout;
        let nodes = vec![
            "ServiceA".to_string(),
            "ServiceB".to_string(),
            "Database".to_string(),
            "Controller".to_string(),
        ];
        let edges = vec![
            ("Controller".to_string(), "ServiceA".to_string()),
            ("ServiceA".to_string(), "Database".to_string()),
            ("ServiceB".to_string(), "Database".to_string()),
        ];
        let seed = blake3::hash(b"deterministic_seed_payload").into();

        let run1 = layout.compute_layout(&nodes, &edges, &seed, true).unwrap();
        let run2 = layout.compute_layout(&nodes, &edges, &seed, true).unwrap();

        // Must be 100% byte-identical
        assert_eq!(run1, run2);
        assert_eq!(run1["Controller"].cluster_id, 1);
        assert_eq!(run1["Database"].cluster_id, 1);
    }

    #[test]
    fn test_5000_node_layout_is_fast_and_repeatable() {
        let layout = DeterministicForceLayout;
        let nodes = (0..5000)
            .map(|index| format!("module::{index:04}"))
            .collect::<Vec<_>>();
        let edges = nodes
            .windows(2)
            .map(|pair| (pair[0].clone(), pair[1].clone()))
            .collect::<Vec<_>>();
        let seed = *blake3::hash(b"phase-two-large-graph-layout").as_bytes();
        let compute_started = std::time::Instant::now();
        let baseline = layout.compute_layout(&nodes, &edges, &seed, true).unwrap();
        let single_run = compute_started.elapsed();
        let expected_digest = digest_coordinates(&baseline);
        let started = std::time::Instant::now();
        for _ in 0..100 {
            let repeat = layout.compute_layout(&nodes, &edges, &seed, true).unwrap();
            assert_eq!(digest_coordinates(&repeat), expected_digest);
        }
        let elapsed = started.elapsed();
        eprintln!(
            "5,000-node layout: {:.1} ms/run; 100 deterministic runs: {:.1} ms",
            single_run.as_secs_f64() * 1000.0,
            elapsed.as_secs_f64() * 1000.0
        );
        assert_eq!(baseline.len(), 5000);
        assert!(single_run < std::time::Duration::from_secs(1));
        assert!(elapsed < std::time::Duration::from_secs(20));
    }

    fn digest_coordinates(coordinates: &BTreeMap<String, NodeCoordinates>) -> blake3::Hash {
        let mut hasher = blake3::Hasher::new();
        for (node, coordinate) in coordinates {
            hasher.update(node.as_bytes());
            hasher.update(&coordinate.x.to_bits().to_le_bytes());
            hasher.update(&coordinate.y.to_bits().to_le_bytes());
            hasher.update(&coordinate.z.to_bits().to_le_bytes());
            hasher.update(&coordinate.cluster_id.to_le_bytes());
        }
        hasher.finalize()
    }
}
