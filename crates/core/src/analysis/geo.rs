//! Local east/north offsets from the pad <-> latitude/longitude, and compass helpers.
//!
//! Flights cover a few kilometres at most, so a local tangent-plane approximation on a spherical Earth is accurate
//! to well under a metre; it matches what rocketviz.py and the web viewer have always used.

pub const EARTH_RADIUS_M: f64 = 6_371_000.0;

const COMPASS: [&str; 16] =
    ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE", "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"];

/// A geographic position in degrees.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LatLon {
    pub lat: f64,
    pub lon: f64,
}

/// The point `e` metres east and `n` metres north of `origin`.
pub fn offset(origin: LatLon, e: f64, n: f64) -> LatLon {
    LatLon {
        lat: origin.lat + (n / EARTH_RADIUS_M).to_degrees(),
        lon: origin.lon + (e / (EARTH_RADIUS_M * origin.lat.to_radians().cos())).to_degrees(),
    }
}

/// Metres (east, north) from `origin` to `p`; the inverse of [`offset`].
pub fn local(origin: LatLon, p: LatLon) -> (f64, f64) {
    let n = (p.lat - origin.lat).to_radians() * EARTH_RADIUS_M;
    let e = (p.lon - origin.lon).to_radians() * EARTH_RADIUS_M * origin.lat.to_radians().cos();
    (e, n)
}

/// Compass bearing in degrees [0, 360) of the direction (e, n): 0 = north, 90 = east.
pub fn bearing(e: f64, n: f64) -> f64 {
    e.atan2(n).to_degrees().rem_euclid(360.0)
}

/// 16-point compass name for a bearing.
pub fn compass(bearing_deg: f64) -> &'static str {
    COMPASS[(((bearing_deg + 11.25).rem_euclid(360.0)) / 22.5) as usize % 16]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn offset_and_local_round_trip() {
        let pad = LatLon { lat: 38.95776, lon: -104.706208 };
        let p = offset(pad, -195.7, -14.0);
        let (e, n) = local(pad, p);
        assert!((e + 195.7).abs() < 1e-9 && (n + 14.0).abs() < 1e-9);
    }

    #[test]
    fn matches_the_python_formula() {
        // rocketviz.py: lat0 + degrees(n / R), lon0 + degrees(e / (R cos(lat0)))
        let p = offset(LatLon { lat: 38.95776, lon: -104.706208 }, 100.0, 50.0);
        assert!((p.lat - (38.95776 + (50.0f64 / 6_371_000.0).to_degrees())).abs() < 1e-12);
        assert!(
            (p.lon - (-104.706208 + (100.0 / (6_371_000.0 * 38.95776f64.to_radians().cos())).to_degrees())).abs()
                < 1e-12
        );
    }

    #[test]
    fn compass_points() {
        assert_eq!(compass(bearing(0.0, 1.0)), "N");
        assert_eq!(compass(bearing(1.0, 0.0)), "E");
        assert_eq!(compass(bearing(-1.0, 0.0)), "W");
        assert_eq!(compass(bearing(-1.0, -1.0)), "SW");
        assert_eq!(compass(359.0), "N");
        assert_eq!(compass(348.74), "NNW");
    }
}
