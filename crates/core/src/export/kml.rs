//! Google Earth KML from a flight file: per config, the flight path by phase, an animated track, event markers, the
//! landing point and recovery walk, Monte Carlo landings with 1/2/3-sigma zones, and the wind sweep. A port of
//! rocketviz.py's KML output.

use std::fmt::Write as _;

use crate::analysis::flight_file::{FlightFile, SimRecord};
use crate::analysis::geo::{bearing, compass, offset, LatLon};
use crate::analysis::stats::{percentile_linear, LandingStats};

/// KML colours are aabbggrr: (line, polygon fill, label).
const PHASES: [(&str, &str, &str, &str); 3] = [
    ("boost", "ff1f6fff", "551f6fff", "Boost (motor burning)"),
    ("coast", "ff00d7ff", "4400d7ff", "Coast to apogee"),
    ("descent", "ffffc040", "33ffc040", "Descent under recovery"),
];

const EVENT_LABELS: [(&str, &str); 5] = [
    ("BURNOUT", "Burnout"),
    ("APOGEE", "Apogee"),
    ("RECOVERY_DEVICE_DEPLOYMENT", "Chute deploy"),
    ("GROUND_HIT", "Landing"),
    ("LAUNCHROD", "Clears rod"),
];

/// Seconds since midnight of the track's clock (Google Earth needs absolute times; the date is arbitrary).
const TRACK_START: &str = "2026-01-01";
const TRACK_START_SECONDS: f64 = 12.0 * 3600.0;

/// The whole flight file as a KML document.
pub fn to_kml(file: &FlightFile) -> String {
    let pad = file.pad;
    let mut out = String::with_capacity(1 << 20);
    let _ = write!(
        out,
        r#"<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2" xmlns:gx="http://www.google.com/kml/ext/2.2">
<Document><name>{} — rocketviz flight</name><open>1</open>
<LookAt><longitude>{}</longitude><latitude>{}</latitude><altitude>0</altitude>
<heading>0</heading><tilt>60</tilt><range>1500</range><altitudeMode>relativeToGround</altitudeMode></LookAt>
{}
"#,
        escape(&file.rocket),
        pad.lon,
        pad.lat,
        styles()
    );
    let pad_desc = format!(
        "Launch site {:.5}, {:.5}<br>Site altitude used by the simulation (air density): {:.0} m MSL",
        pad.lat, pad.lon, file.site_alt
    );
    out.push_str(&point("Launch pad", pad, 0.0, "pad", &pad_desc, "clampToGround"));
    out.push('\n');
    for (i, sim) in file.sims.iter().enumerate() {
        out.push_str(&sim_folder(sim, i == 0));
    }
    out.push_str("\n</Document></kml>");
    out
}

fn sim_folder(sim: &SimRecord, open: bool) -> String {
    let site = LatLon { lat: sim.conditions.lat, lon: sim.conditions.lon };
    let lls: Vec<LatLon> = sim.e.iter().zip(&sim.n).map(|(&e, &n)| offset(site, e, n)).collect();
    let (le, ln) = (*sim.e.last().unwrap_or(&0.0), *sim.n.last().unwrap_or(&0.0));
    let (dist, brg) = (le.hypot(ln), bearing(le, ln));
    let land = *lls.last().unwrap_or(&site);
    let summary = summary_html(sim, dist, brg);
    let mut parts = String::new();

    // Flight path, coloured by phase.
    parts.push_str("<Folder><name>Flight path</name>");
    for ((key, _, _, label), (a, b)) in PHASES.iter().zip(phase_ranges(sim)) {
        if b > a {
            parts.push_str(&line(
                label,
                &lls[a..=b],
                &sim.alt[a..=b],
                &format!("ph_{key}"),
                true,
                "relativeToGround",
                "",
            ));
        }
    }
    parts.push_str("</Folder>");

    // Animated track.
    let mut whens = String::new();
    let mut coords = String::new();
    for ((t, p), a) in sim.t.iter().zip(&lls).zip(&sim.alt) {
        let _ = write!(whens, "<when>{}</when>", timestamp(*t));
        let _ = write!(coords, "<gx:coord>{:.7} {:.7} {:.2}</gx:coord>", p.lon, p.lat, a.max(0.0));
    }
    let _ = write!(
        parts,
        "<Placemark><name>Rocket (press play on time slider)</name><styleUrl>#rocket</styleUrl>\n<gx:Track><altitudeMode>relativeToGround</altitudeMode>{whens}{coords}</gx:Track></Placemark>"
    );

    // Events.
    let speed = sim.series.iter().find(|s| s.key == "total_velocity");
    parts.push_str("<Folder><name>Events</name>");
    for ev in &sim.events {
        let Some((_, label)) = EVENT_LABELS.iter().find(|(k, _)| *k == ev.kind) else { continue };
        if ev.kind == "GROUND_HIT" {
            continue;
        }
        let i = nearest_index(&sim.t, ev.t);
        let v = speed.and_then(|s| s.data.get(i).copied().flatten()).unwrap_or(f64::NAN);
        let desc = format!("Altitude {:.0} m AGL<br>Velocity {:.1} m/s", sim.alt[i], v);
        parts.push_str(&point(
            &format!("{label} T+{:.1}s", ev.t),
            lls[i],
            sim.alt[i],
            "event",
            &desc,
            "relativeToGround",
        ));
    }
    parts.push_str("</Folder>");

    // Landing and recovery walk.
    parts.push_str(&point(
        &format!("Landing: {dist:.0} m {}", compass(brg)),
        land,
        0.0,
        "land",
        &summary,
        "clampToGround",
    ));
    parts.push_str(&line(
        &format!("Recovery walk {}", fmt_dist(dist)),
        &[site, land],
        &[0.0, 0.0],
        "walk",
        false,
        "clampToGround",
        "",
    ));

    // Monte Carlo.
    let pts: Vec<[f64; 2]> = sim.mc.iter().map(|m| [m.e, m.n]).collect();
    if let Some(stats) = LandingStats::from_points(&pts) {
        let mut ells = String::new();
        for (k, pct) in [(1.0, "39.3"), (2.0, "86.5"), (3.0, "98.9")] {
            let ring: Vec<LatLon> = stats.ellipse(k, 72).iter().map(|p| offset(site, p[0], p[1])).collect();
            let desc = format!("Semi-axes {:.0} m × {:.0} m", k * stats.semi_axes[0], k * stats.semi_axes[1]);
            ells.push_str(&polygon(
                &format!("{k:.0}σ landing zone (~{pct}% of landings)"),
                &ring,
                &format!("ell{k:.0}"),
                &desc,
            ));
        }
        let mut dots = String::new();
        for (i, m) in sim.mc.iter().enumerate() {
            let desc = format!(
                "wind {:.1} m/s from {:.0}°<br>apogee {:.0} m<br>drift {:.0} m",
                m.wind,
                m.dir,
                m.apogee,
                m.e.hypot(m.n)
            );
            dots.push_str(&point(&format!("MC {i}"), offset(site, m.e, m.n), 0.0, "mc", &desc, "clampToGround"));
        }
        let mut drift: Vec<f64> = pts.iter().map(|p| p[0].hypot(p[1])).collect();
        drift.sort_by(f64::total_cmp);
        let (lo, hi) =
            sim.mc.iter().fold((f64::INFINITY, f64::NEG_INFINITY), |(lo, hi), m| (lo.min(m.apogee), hi.max(m.apogee)));
        let text = format!(
            "{} runs. Drift: median {:.0} m, 90th pct {:.0} m, max {:.0} m. Apogee {lo:.0}–{hi:.0} m.",
            sim.mc.len(),
            stats.drift.median,
            percentile_linear(&drift, 0.9),
            stats.drift.max
        );
        let _ = write!(
            parts,
            "<Folder><name>Monte Carlo dispersion</name><description>{}</description>{ells}<Folder><name>Landing points</name>{dots}</Folder></Folder>",
            escape(&text)
        );
    }

    // Wind sweep.
    if !sim.sweep.is_empty() {
        let mut sp = String::new();
        let mut path = vec![site];
        for w in &sim.sweep {
            let p = offset(site, w.e, w.n);
            path.push(p);
            let d = w.e.hypot(w.n);
            let desc = format!("Wind {:.1} m/s ({:.0} mph): lands {}", w.wind, w.wind * 2.237, fmt_dist(d));
            sp.push_str(&point(&format!("{:.0} m/s → {d:.0} m", w.wind), p, 0.0, "sweep", &desc, "clampToGround"));
        }
        sp.push_str(&line("Drift line", &path, &vec![0.0; path.len()], "sweepline", false, "clampToGround", ""));
        let _ =
            write!(parts, "<Folder><name>Wind sweep (same direction)</name><visibility>0</visibility>{sp}</Folder>");
    }

    format!(
        "<Folder><name>{}</name><open>{}</open><description><![CDATA[{summary}]]></description>{parts}</Folder>",
        escape(&sim.name),
        u8::from(open)
    )
}

/// Sample index ranges (inclusive) for boost, coast and descent, as rocketviz.py computed them.
fn phase_ranges(sim: &SimRecord) -> [(usize, usize); 3] {
    let ev = |k: &str| sim.events.iter().find(|e| e.kind == k).map(|e| e.t);
    let liftoff = ev("LIFTOFF").unwrap_or(0.0);
    let burnout = ev("BURNOUT").unwrap_or(liftoff);
    let deploy = ev("RECOVERY_DEVICE_DEPLOYMENT").or(ev("APOGEE")).unwrap_or(burnout);
    let (i0, i1, i2) = (nearest_index(&sim.t, liftoff), nearest_index(&sim.t, burnout), nearest_index(&sim.t, deploy));
    [(i0, i1), (i1, i2), (i2, sim.t.len().saturating_sub(1))]
}

/// Index of the sample closest in time to `t` (first one on ties).
fn nearest_index(ts: &[f64], t: f64) -> usize {
    let mut best = 0;
    for (i, x) in ts.iter().enumerate() {
        if (x - t).abs() < (ts[best] - t).abs() {
            best = i;
        }
    }
    best
}

/// `2026-01-01T12:00:05.123Z` for t = 5.1234 s (milliseconds truncated, as rocketviz.py did).
fn timestamp(t: f64) -> String {
    let micros = ((TRACK_START_SECONDS + t) * 1e6).round() as i64;
    let ms = micros / 1000;
    let (h, m, s, ms) = (ms / 3_600_000, ms / 60_000 % 60, ms / 1000 % 60, ms % 1000);
    format!("{TRACK_START}T{h:02}:{m:02}:{s:02}.{ms:03}Z")
}

fn summary_html(sim: &SimRecord, dist: f64, brg: f64) -> String {
    let (s, c) = (&sim.summary, &sim.conditions);
    let mach = sim
        .series
        .iter()
        .find(|x| x.key == "mach_number")
        .map(|x| x.data.iter().flatten().fold(f64::NEG_INFINITY, |a, &b| a.max(b)))
        .unwrap_or(f64::NAN);
    let rows = [
        ("Apogee", fmt_dist(s.max_alt)),
        ("Max velocity", format!("{:.1} m/s (Mach {mach:.2})", s.max_v)),
        ("Time to apogee", format!("{:.1} s", s.apogee_t)),
        ("Flight time", format!("{:.1} s", s.flight_t)),
        ("Off-rod velocity", format!("{:.1} m/s", s.rod_v)),
        ("Landing velocity", format!("{:.1} m/s", s.ground_v)),
        ("Drift to landing", format!("{} toward {brg:.0}° {}", fmt_dist(dist), compass(brg))),
        (
            "Wind",
            format!(
                "{:.1} m/s from {:.0}° {}, turbulence {:.0}%",
                c.wind_speed,
                c.wind_dir_deg,
                compass(c.wind_dir_deg),
                c.turbulence * 100.0
            ),
        ),
        ("Rod", format!("{:.2} m, {:.1}° tilt", c.rod_length, c.rod_angle_deg)),
    ];
    let mut html = format!("<b>{}</b><table>", escape(&sim.name));
    for (k, v) in rows {
        let _ = write!(html, "<tr><td><b>{k}</b></td><td>{v}</td></tr>");
    }
    html.push_str("</table>");
    html
}

/// "1,234 m (4,049 ft)"
fn fmt_dist(m: f64) -> String {
    format!("{} m ({} ft)", thousands(m), thousands(m * 3.28084))
}

fn thousands(x: f64) -> String {
    let digits = format!("{:.0}", x.abs());
    let mut out = String::new();
    for (i, ch) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    if x.round() < 0.0 {
        format!("-{out}")
    } else {
        out
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;")
}

fn point(name: &str, p: LatLon, alt: f64, style: &str, desc: &str, mode: &str) -> String {
    format!(
        "<Placemark><name>{}</name>\n<styleUrl>#{style}</styleUrl><description><![CDATA[{desc}]]></description>\n<Point><altitudeMode>{mode}</altitudeMode><coordinates>{:.7},{:.7},{alt:.2}</coordinates></Point></Placemark>",
        escape(name),
        p.lon,
        p.lat
    )
}

fn line(name: &str, pts: &[LatLon], alts: &[f64], style: &str, extrude: bool, mode: &str, desc: &str) -> String {
    let coords: Vec<String> =
        pts.iter().zip(alts).map(|(p, a)| format!("{:.7},{:.7},{:.2}", p.lon, p.lat, a.max(0.0))).collect();
    format!(
        "<Placemark><name>{}</name>\n<styleUrl>#{style}</styleUrl><description><![CDATA[{desc}]]></description>\n<LineString><extrude>{}</extrude><tessellate>1</tessellate>\n<altitudeMode>{mode}</altitudeMode><coordinates>{}</coordinates></LineString></Placemark>",
        escape(name),
        u8::from(extrude),
        coords.join(" ")
    )
}

fn polygon(name: &str, ring: &[LatLon], style: &str, desc: &str) -> String {
    let mut coords: Vec<String> = ring.iter().map(|p| format!("{:.7},{:.7},0", p.lon, p.lat)).collect();
    if let Some(first) = coords.first().cloned() {
        coords.push(first);
    }
    format!(
        "<Placemark><name>{}</name><styleUrl>#{style}</styleUrl>\n<description><![CDATA[{desc}]]></description>\n<Polygon><tessellate>1</tessellate><altitudeMode>clampToGround</altitudeMode>\n<outerBoundaryIs><LinearRing><coordinates>{}</coordinates></LinearRing></outerBoundaryIs></Polygon></Placemark>",
        escape(name),
        coords.join(" ")
    )
}

fn styles() -> String {
    let icon = "http://maps.google.com/mapfiles/kml/";
    let mut s: Vec<String> = PHASES
        .iter()
        .map(|(key, line, poly, _)| {
            format!("<Style id=\"ph_{key}\"><LineStyle><color>{line}</color><width>4</width></LineStyle><PolyStyle><color>{poly}</color><outline>0</outline></PolyStyle></Style>")
        })
        .collect();
    s.extend([
        format!("<Style id=\"pad\"><IconStyle><scale>1.3</scale><Icon><href>{icon}shapes/triangle.png</href></Icon><color>ff00ff00</color></IconStyle><LabelStyle><scale>1</scale></LabelStyle></Style>"),
        format!("<Style id=\"land\"><IconStyle><scale>1.3</scale><Icon><href>{icon}shapes/target.png</href></Icon><color>ff0000ff</color></IconStyle></Style>"),
        format!("<Style id=\"event\"><IconStyle><scale>0.8</scale><Icon><href>{icon}shapes/placemark_circle.png</href></Icon></IconStyle><LabelStyle><scale>0.8</scale></LabelStyle></Style>"),
        format!("<Style id=\"mc\"><IconStyle><scale>0.45</scale><color>ccffffff</color><Icon><href>{icon}shapes/shaded_dot.png</href></Icon></IconStyle><LabelStyle><scale>0</scale></LabelStyle></Style>"),
        format!("<Style id=\"sweep\"><IconStyle><scale>0.7</scale><color>ff00ffff</color><Icon><href>{icon}shapes/open-diamond.png</href></Icon></IconStyle><LabelStyle><scale>0.7</scale></LabelStyle></Style>"),
        "<Style id=\"walk\"><LineStyle><color>ff0000ff</color><width>3</width></LineStyle></Style>".into(),
        "<Style id=\"sweepline\"><LineStyle><color>9900ffff</color><width>2</width></LineStyle></Style>".into(),
        "<Style id=\"ell1\"><LineStyle><color>ff00ff00</color><width>2</width></LineStyle><PolyStyle><color>3300ff00</color></PolyStyle></Style>".into(),
        "<Style id=\"ell2\"><LineStyle><color>ff00ffff</color><width>2</width></LineStyle><PolyStyle><color>2200ffff</color></PolyStyle></Style>".into(),
        "<Style id=\"ell3\"><LineStyle><color>ff0000ff</color><width>2</width></LineStyle><PolyStyle><color>110000ff</color></PolyStyle></Style>".into(),
        format!("<Style id=\"rocket\"><IconStyle><scale>1.1</scale><Icon><href>{icon}shapes/airports.png</href></Icon></IconStyle></Style>"),
    ]);
    s.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_python() {
        assert_eq!(thousands(1234.4), "1,234");
        assert_eq!(thousands(999.6), "1,000");
        assert_eq!(thousands(12.0), "12");
        assert_eq!(fmt_dist(152.3), "152 m (500 ft)");
        assert_eq!(timestamp(0.0), "2026-01-01T12:00:00.000Z");
        assert_eq!(timestamp(5.4239), "2026-01-01T12:00:05.423Z");
        assert_eq!(timestamp(61.0), "2026-01-01T12:01:01.000Z");
        assert_eq!(escape("a <b> & c"), "a &lt;b&gt; &amp; c");
    }
}
