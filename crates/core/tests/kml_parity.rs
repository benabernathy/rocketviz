//! KML export compared with rocketviz.py's KML for the same analysis: web/public/samples/fizz.rocketviz.json and
//! tests/fixtures/fizz-python.kml were written by the same rocketviz.py run.
//!
//! Same structure, same placemark names, points and lines at the same places (to the last printed digit). The ellipse
//! rings are compared by area and centroid: same ellipse, different starting point.

use rocketviz_core::analysis::flight_file::FlightFile;
use rocketviz_core::export::kml::to_kml;

const KML_NS: &str = "http://www.opengis.net/kml/2.2";

#[derive(Debug)]
struct Placemark {
    path: String,
    name: String,
    coords: Vec<[f64; 3]>,
    kind: &'static str,
}

fn placemarks(kml: &str) -> Vec<Placemark> {
    let doc = roxmltree::Document::parse(kml).expect("well-formed KML");
    let mut out = Vec::new();
    for pm in doc.descendants().filter(|n| n.has_tag_name((KML_NS, "Placemark"))) {
        let name =
            pm.children().find(|c| c.has_tag_name((KML_NS, "name"))).and_then(|n| n.text()).unwrap_or("").to_string();
        let path = pm
            .ancestors()
            .filter(|a| a.has_tag_name((KML_NS, "Folder")))
            .filter_map(|f| f.children().find(|c| c.has_tag_name((KML_NS, "name"))).and_then(|n| n.text()))
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join(" / ");
        let (kind, coords) = if let Some(track) = pm.descendants().find(|n| n.tag_name().name() == "Track") {
            let c = track
                .children()
                .filter(|n| n.tag_name().name() == "coord")
                .map(|n| {
                    let v: Vec<f64> = n.text().unwrap().split_whitespace().map(|x| x.parse().unwrap()).collect();
                    [v[0], v[1], v[2]]
                })
                .collect();
            ("track", c)
        } else {
            let kind = if pm.descendants().any(|n| n.has_tag_name((KML_NS, "Polygon"))) {
                "polygon"
            } else if pm.descendants().any(|n| n.has_tag_name((KML_NS, "LineString"))) {
                "line"
            } else {
                "point"
            };
            let text =
                pm.descendants().find(|n| n.has_tag_name((KML_NS, "coordinates"))).and_then(|n| n.text()).unwrap_or("");
            let c = text
                .split_whitespace()
                .map(|t| {
                    let v: Vec<f64> = t.split(',').map(|x| x.parse().unwrap()).collect();
                    [v[0], v[1], *v.get(2).unwrap_or(&0.0)]
                })
                .collect();
            (kind, c)
        };
        out.push(Placemark { path, name, coords, kind });
    }
    out
}

/// Distance in metres between two (lon, lat) points (local approximation; fine at these scales).
fn metres(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dn = (a[1] - b[1]).to_radians() * 6_371_000.0;
    let de = (a[0] - b[0]).to_radians() * 6_371_000.0 * a[1].to_radians().cos();
    de.hypot(dn)
}

/// Polygon area (m^2) and centroid, for comparing ellipse rings that start at different points.
fn area_centroid(ring: &[[f64; 3]]) -> (f64, [f64; 3]) {
    let o = ring[0];
    let xy: Vec<(f64, f64)> = ring
        .iter()
        .map(|p| {
            (
                (p[0] - o[0]).to_radians() * 6_371_000.0 * o[1].to_radians().cos(),
                (p[1] - o[1]).to_radians() * 6_371_000.0,
            )
        })
        .collect();
    let (mut a, mut cx, mut cy) = (0.0, 0.0, 0.0);
    for w in xy.windows(2) {
        let cross = w[0].0 * w[1].1 - w[1].0 * w[0].1;
        a += cross;
        cx += (w[0].0 + w[1].0) * cross;
        cy += (w[0].1 + w[1].1) * cross;
    }
    let a = a / 2.0;
    let (cx, cy) = (cx / (6.0 * a), cy / (6.0 * a));
    let lat = o[1] + (cy / 6_371_000.0).to_degrees();
    let lon = o[0] + (cx / (6_371_000.0 * o[1].to_radians().cos())).to_degrees();
    (a.abs(), [lon, lat, 0.0])
}

#[test]
fn matches_rocketviz_py_kml() {
    let root = concat!(env!("CARGO_MANIFEST_DIR"), "/../..");
    let file: FlightFile = serde_json::from_str(
        &std::fs::read_to_string(format!("{root}/web/public/samples/fizz.rocketviz.json")).unwrap(),
    )
    .unwrap();
    let rust = placemarks(&to_kml(&file));
    let python = placemarks(include_str!("fixtures/fizz-python.kml"));

    assert_eq!(rust.len(), python.len(), "placemark count");
    let mut worst_point = 0.0f64;
    let mut worst_track = 0.0f64;
    for (r, p) in rust.iter().zip(&python) {
        assert_eq!(r.path, p.path, "folder of {:?}", p.name);
        assert_eq!(r.name, p.name, "placemark name in {}", p.path);
        assert_eq!(r.kind, p.kind, "{}", p.name);
        assert_eq!(r.coords.len(), p.coords.len(), "{}: coordinate count", p.name);
        match r.kind {
            "polygon" => {
                let ((ra, rc), (pa, pc)) = (area_centroid(&r.coords), area_centroid(&p.coords));
                assert!((ra - pa).abs() / pa < 0.01, "{}: area {ra:.0} vs {pa:.0} m²", p.name);
                assert!(metres(rc, pc) < 0.1, "{}: centroid off by {:.3} m", p.name, metres(rc, pc));
            }
            "track" => {
                for (a, b) in r.coords.iter().zip(&p.coords) {
                    worst_track = worst_track.max(metres(*a, *b));
                    assert!((a[2] - b[2]).abs() < 0.011, "{}: altitude {} vs {}", p.name, a[2], b[2]);
                }
            }
            _ => {
                for (a, b) in r.coords.iter().zip(&p.coords) {
                    let d = metres(*a, *b);
                    let on_track = p.path.ends_with("Flight path")
                        || p.path.ends_with("Events")
                        || p.name.starts_with("Landing")
                        || p.name.starts_with("Recovery walk");
                    if on_track {
                        worst_track = worst_track.max(d);
                    } else {
                        worst_point = worst_point.max(d);
                    }
                    assert!((a[2] - b[2]).abs() < 0.011, "{}: altitude {} vs {}", p.name, a[2], b[2]);
                }
            }
        }
    }
    eprintln!("{} placemarks match; Monte Carlo / sweep / pad points within {worst_point:.4} m; flight track within {worst_track:.3} m", rust.len());
    // Both write coordinates with 7 decimals (1e-7 degrees ~ 1.1 cm), so one or two quanta of difference is just
    // formatting. (OpenRocket's own track lat/lon turns out to use the same local offset as the core.)
    assert!(worst_point < 0.025, "pad-relative points: {worst_point} m");
    assert!(worst_track < 0.025, "flight track vs OpenRocket's own lat/lon: {worst_track} m");
}
