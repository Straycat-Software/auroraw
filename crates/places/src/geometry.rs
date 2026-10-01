// SPDX-License-Identifier: GPL-3.0-or-later
//! The arithmetic of the lookup: rings stored as integers, the crossing-number test, distances.
//!
//! Positions are degrees of longitude and latitude; the polygons of Natural Earth are planar in those
//! coordinates (their edges are straight lines on the plate carrée), so the tests are planar too.

/// Stored coordinates are whole units of 1e-5 degree, about a metre: far finer than the 1:10 million
/// polygons they hold, and small enough for a delta to fit in two bytes.
pub(crate) const SCALE: f64 = 100_000.0;

/// Metres in a degree of latitude (and of longitude on the equator) on a sphere of the mean radius.
const METRES_PER_DEGREE: f64 = 111_194.9;

/// The mean radius of the Earth, in metres.
const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// One ring of a polygon: `(longitude, latitude)` in units of [`SCALE`].
pub(crate) type Ring = Vec<(i32, i32)>;

/// The smallest box that holds the rings, in degrees: `(min_lon, max_lon, min_lat, max_lat)`.
pub(crate) type BoundingBox = (f64, f64, f64, f64);

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value & 0x7f) as u8 | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn get_varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0_u64;
    let mut shift = 0;
    loop {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

fn zigzag(value: i64) -> u64 {
    ((value << 1) ^ (value >> 63)) as u64
}

fn unzigzag(value: u64) -> i64 {
    (value >> 1) as i64 ^ -((value & 1) as i64)
}

/// The rings of one polygon part, as bytes: the number of rings, then for each the number of points and
/// every point as the zigzag varint difference from the one before (the first from the origin).
pub(crate) fn encode(rings: &[Vec<(f64, f64)>]) -> Vec<u8> {
    let mut out = Vec::new();
    put_varint(&mut out, rings.len() as u64);
    for ring in rings {
        put_varint(&mut out, ring.len() as u64);
        let (mut last_x, mut last_y) = (0_i64, 0_i64);
        for &(lon, lat) in ring {
            let x = (lon * SCALE).round() as i64;
            let y = (lat * SCALE).round() as i64;
            put_varint(&mut out, zigzag(x - last_x));
            put_varint(&mut out, zigzag(y - last_y));
            (last_x, last_y) = (x, y);
        }
    }
    out
}

/// The rings of [`encode`]'s bytes, or `None` for bytes that are not a valid encoding.
pub(crate) fn decode(bytes: &[u8]) -> Option<Vec<Ring>> {
    let mut at = 0;
    let rings = get_varint(bytes, &mut at)?;
    let mut out = Vec::new();
    for _ in 0..rings {
        let points = get_varint(bytes, &mut at)?;
        if points > bytes.len() as u64 {
            return None;
        }
        let mut ring = Vec::with_capacity(points as usize);
        let (mut x, mut y) = (0_i64, 0_i64);
        for _ in 0..points {
            x += unzigzag(get_varint(bytes, &mut at)?);
            y += unzigzag(get_varint(bytes, &mut at)?);
            ring.push((i32::try_from(x).ok()?, i32::try_from(y).ok()?));
        }
        out.push(ring);
    }
    (at == bytes.len()).then_some(out)
}

/// The box of the rings, in degrees.
pub(crate) fn bounding_box(rings: &[Vec<(f64, f64)>]) -> BoundingBox {
    let mut bounds = (f64::MAX, f64::MIN, f64::MAX, f64::MIN);
    for &(lon, lat) in rings.iter().flatten() {
        bounds.0 = bounds.0.min(lon);
        bounds.1 = bounds.1.max(lon);
        bounds.2 = bounds.2.min(lat);
        bounds.3 = bounds.3.max(lat);
    }
    bounds
}

/// Whether the point is inside the rings by the even-odd rule: a point is inside when a ray to the east
/// crosses an odd number of edges of all the rings together, so a hole inside the outer ring counts as
/// outside, and an island inside a hole as inside.
pub(crate) fn contains(rings: &[Ring], lon: f64, lat: f64) -> bool {
    let (x, y) = (lon * SCALE, lat * SCALE);
    let mut inside = false;
    for ring in rings {
        let Some(&last) = ring.last() else {
            continue;
        };
        let (mut x1, mut y1) = (f64::from(last.0), f64::from(last.1));
        for &(px, py) in ring {
            let (x2, y2) = (f64::from(px), f64::from(py));
            if (y1 > y) != (y2 > y) {
                let crossing = x1 + (y - y1) / (y2 - y1) * (x2 - x1);
                if x < crossing {
                    inside = !inside;
                }
            }
            (x1, y1) = (x2, y2);
        }
    }
    inside
}

/// The distance in metres from the point to the nearest edge of the rings, measured on a plane tangent at
/// the point (exact enough for a few kilometres, which is all it is asked for).
pub(crate) fn distance_m(rings: &[Ring], lon: f64, lat: f64) -> f64 {
    let east = lat.to_radians().cos() * METRES_PER_DEGREE;
    let north = METRES_PER_DEGREE;
    let local = |(px, py): (i32, i32)| {
        let mut dlon = f64::from(px) / SCALE - lon;
        if dlon > 180.0 {
            dlon -= 360.0;
        } else if dlon < -180.0 {
            dlon += 360.0;
        }
        (dlon * east, (f64::from(py) / SCALE - lat) * north)
    };
    let mut best = f64::MAX;
    for ring in rings {
        let Some(&first) = ring.first() else {
            continue;
        };
        let mut from = local(first);
        for &point in &ring[1..] {
            let to = local(point);
            best = best.min(segment_distance(from, to));
            from = to;
        }
        best = best.min(segment_distance(from, local(first)));
    }
    best
}

/// The distance from the origin to the segment `a`-`b`.
fn segment_distance(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let length_squared = dx * dx + dy * dy;
    let t = if length_squared == 0.0 {
        0.0
    } else {
        (-(a.0 * dx + a.1 * dy) / length_squared).clamp(0.0, 1.0)
    };
    let (x, y) = (a.0 + t * dx, a.1 + t * dy);
    x.hypot(y)
}

/// The great-circle distance in metres between two positions.
pub(crate) fn haversine_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().asin()
}

/// The longitudes `[lon - reach, lon + reach]` as one or two ranges inside `[-180, 180]`: a search box
/// that crosses the antimeridian is two boxes.
pub(crate) fn longitude_ranges(lon: f64, reach: f64) -> Vec<(f64, f64)> {
    if reach >= 180.0 {
        return vec![(-180.0, 180.0)];
    }
    let (low, high) = (lon - reach, lon + reach);
    if low < -180.0 {
        vec![(-180.0, high), (low + 360.0, 180.0)]
    } else if high > 180.0 {
        vec![(low, 180.0), (-180.0, high - 360.0)]
    } else {
        vec![(low, high)]
    }
}

/// How many degrees of longitude are `metres` at this latitude (capped near the poles, where a box of
/// the whole circle of latitude is the honest answer).
pub(crate) fn degrees_of_longitude(metres: f64, lat: f64) -> f64 {
    let scale = lat.to_radians().cos().max(0.01);
    (metres / (METRES_PER_DEGREE * scale)).min(180.0)
}

/// How many degrees of latitude are `metres`.
pub(crate) fn degrees_of_latitude(metres: f64) -> f64 {
    metres / METRES_PER_DEGREE
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(x0: f64, x1: f64, y0: f64, y1: f64) -> Vec<(f64, f64)> {
        vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1), (x0, y0)]
    }

    fn rings(list: &[Vec<(f64, f64)>]) -> Vec<Ring> {
        decode(&encode(list)).expect("a valid encoding")
    }

    #[test]
    fn rings_survive_the_encoding_to_the_metre() {
        let mut random = fastrand::Rng::with_seed(7);
        for _ in 0..200 {
            let original: Vec<Vec<(f64, f64)>> = (0..random.usize(1..4))
                .map(|_| {
                    (0..random.usize(0..60))
                        .map(|_| (random.f64() * 360.0 - 180.0, random.f64() * 180.0 - 90.0))
                        .collect()
                })
                .collect();
            let back = decode(&encode(&original)).expect("decodes");
            assert_eq!(back.len(), original.len());
            for (ring, source) in back.iter().zip(&original) {
                assert_eq!(ring.len(), source.len());
                for (&(x, y), &(lon, lat)) in ring.iter().zip(source) {
                    assert!((f64::from(x) / SCALE - lon).abs() < 0.000_006);
                    assert!((f64::from(y) / SCALE - lat).abs() < 0.000_006);
                }
            }
        }
    }

    #[test]
    fn bytes_that_are_not_an_encoding_are_refused_not_trusted() {
        assert!(decode(&[0x80]).is_none(), "a varint that never ends");
        assert!(decode(&[1, 200, 1]).is_none(), "more points than bytes");
        let mut good = encode(&[square(0.0, 1.0, 0.0, 1.0)]);
        good.push(0);
        assert!(decode(&good).is_none(), "bytes left over");
        assert_eq!(decode(&encode(&[])), Some(Vec::new()));
    }

    #[test]
    fn a_hole_is_outside_and_an_island_in_the_hole_is_inside() {
        let land = rings(&[
            square(0.0, 10.0, 0.0, 10.0),
            square(3.0, 7.0, 3.0, 7.0),
            square(4.0, 6.0, 4.0, 6.0),
        ]);
        assert!(contains(&land, 1.0, 1.0));
        assert!(!contains(&land, 3.5, 5.0), "in the hole");
        assert!(contains(&land, 5.0, 5.0), "on the island in the hole");
        assert!(!contains(&land, 11.0, 5.0));
        assert!(!contains(&land, 5.0, -0.5));
    }

    #[test]
    fn the_distance_to_a_shore_is_in_metres() {
        let land = rings(&[square(0.0, 1.0, 0.0, 1.0)]);
        // 0.01 degree of latitude south of the bottom edge.
        let d = distance_m(&land, 0.5, -0.01);
        assert!((d - 1_111.9).abs() < 5.0, "{d}");
        // Near a corner, the corner is the nearest.
        let corner = distance_m(&land, -0.01, -0.01);
        assert!((corner - 1_111.9 * 2.0_f64.sqrt()).abs() < 5.0, "{corner}");
        // East-west distance shrinks with latitude: 0.01 degree of longitude at 60.5 north is 547 m.
        let high = rings(&[square(0.0, 1.0, 60.0, 61.0)]);
        let d = distance_m(&high, -0.01, 60.5);
        assert!((d - 547.0).abs() < 3.0, "{d}");
    }

    #[test]
    fn a_search_box_across_the_antimeridian_is_two_boxes() {
        assert_eq!(longitude_ranges(10.0, 1.0), vec![(9.0, 11.0)]);
        assert_eq!(
            longitude_ranges(179.5, 1.0),
            vec![(178.5, 180.0), (-180.0, -179.5)]
        );
        assert_eq!(
            longitude_ranges(-179.5, 1.0),
            vec![(-180.0, -178.5), (179.5, 180.0)]
        );
        assert_eq!(longitude_ranges(0.0, 200.0), vec![(-180.0, 180.0)]);
    }

    #[test]
    fn a_great_circle_distance_is_right_for_two_capitals() {
        // Paris to London: about 344 km.
        let d = haversine_m(48.8566, 2.3522, 51.5074, -0.1278);
        assert!((d - 343_600.0).abs() < 2_000.0, "{d}");
        assert_eq!(haversine_m(10.0, 20.0, 10.0, 20.0), 0.0);
    }
}
