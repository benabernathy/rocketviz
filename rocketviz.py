#!/usr/bin/env python3
"""rocketviz — turn OpenRocket simulations into a Google Earth flight + recovery map.

Usage:
  python3 rocketviz.py Fizz.ork [--lat 38.95776 --lon -80.6] [--mc 150]
                    [--wind 2.0 --wind-from 90] [--sigma-wind 0.6 --sigma-dir 20]
                    [--out Fizz.kml] [--json Fizz.json]

What it produces (KML, opens in Google Earth Pro / earth.google.com):
  * one folder per OpenRocket simulation (motor configuration)
  * animated rocket track (time slider) at true height above the pad
  * phase-coloured trajectory curtain: boost / coast / descent
  * flight event markers (burnout, apogee, deployment, landing)
  * pad -> landing recovery line with distance & bearing
  * Monte Carlo landing scatter + 1/2/3-sigma landing ellipses
  * wind sweep: where it lands at 0..N m/s wind
"""
import argparse
import datetime as dt
import json
import math
import os
import random
import sys
from xml.sax.saxutils import escape

import numpy as np

import orsim

R_EARTH = 6371000.0
HERE = os.path.dirname(os.path.abspath(__file__))

# KML colours are aabbggrr
PHASE_STYLE = {
    "boost":   ("ff1f6fff", "551f6fff", "Boost (motor burning)"),
    "coast":   ("ff00d7ff", "4400d7ff", "Coast to apogee"),
    "descent": ("ffffc040", "33ffc040", "Descent under recovery"),
}
EVENT_LABELS = {
    "BURNOUT": "Burnout",
    "APOGEE": "Apogee",
    "RECOVERY_DEVICE_DEPLOYMENT": "Chute deploy",
    "GROUND_HIT": "Landing",
    "LAUNCHROD": "Clears rod",
}
COMPASS = ["N", "NNE", "NE", "ENE", "E", "ESE", "SE", "SSE",
           "S", "SSW", "SW", "WSW", "W", "WNW", "NW", "NNW"]


# ---------------------------------------------------------------- geometry
def enu_to_ll(lat0, lon0, e, n):
    lat = lat0 + math.degrees(n / R_EARTH)
    lon = lon0 + math.degrees(e / (R_EARTH * math.cos(math.radians(lat0))))
    return lat, lon


def ll_to_enu(lat0, lon0, lat, lon):
    n = math.radians(lat - lat0) * R_EARTH
    e = math.radians(lon - lon0) * R_EARTH * math.cos(math.radians(lat0))
    return e, n


def bearing_deg(e, n):
    return (math.degrees(math.atan2(e, n)) + 360) % 360


def compass(b):
    return COMPASS[int((b + 11.25) // 22.5) % 16]


def fmt_dist(m):
    return f"{m:,.0f} m ({m * 3.28084:,.0f} ft)"


def sigma_ellipse(pts_en, k):
    """Return ellipse (list of (e,n)) at k-sigma for 2-D points."""
    P = np.asarray(pts_en)
    mu = P.mean(axis=0)
    C = np.cov(P.T)
    w, V = np.linalg.eigh(C)
    w = np.clip(w, 1e-9, None)
    th = np.linspace(0, 2 * np.pi, 73)
    circ = np.stack([np.cos(th), np.sin(th)])
    ell = (V @ np.diag(k * np.sqrt(w)) @ circ).T + mu
    return ell, mu, k * np.sqrt(w)


# ---------------------------------------------------------------- trajectory helpers
def georef(traj, lat0=None, lon0=None):
    """Lat/lon per sample. Uses OpenRocket's own (degrees) unless the pad is moved."""
    if lat0 is None:
        return list(zip(traj["lat"], traj["lon"]))
    return [enu_to_ll(lat0, lon0, e, n) for e, n in zip(traj["x"], traj["y"])]


def event_time(traj, name):
    for e in traj["events"]:
        if e["type"] == name:
            return e["t"]
    return None


def idx_at(traj, t):
    ts = traj["t"]
    return min(range(len(ts)), key=lambda i: abs(ts[i] - t))


def phases(traj):
    t_lift = event_time(traj, "LIFTOFF") or 0.0
    t_burn = event_time(traj, "BURNOUT") or t_lift
    t_dep = event_time(traj, "RECOVERY_DEVICE_DEPLOYMENT") or event_time(traj, "APOGEE")
    i0, i1, i2 = idx_at(traj, t_lift), idx_at(traj, t_burn), idx_at(traj, t_dep)
    n = len(traj["t"]) - 1
    return {"boost": (i0, i1), "coast": (i1, i2), "descent": (i2, n)}


def landing(traj):
    return traj["x"][-1], traj["y"][-1]


# ---------------------------------------------------------------- KML builders
def coords(lls, alts):
    return " ".join(f"{lo:.7f},{la:.7f},{max(a, 0):.2f}" for (la, lo), a in zip(lls, alts))


def kml_point(name, lat, lon, alt=0, style="", desc="", mode="relativeToGround", visible=True):
    return f"""<Placemark><name>{escape(name)}</name>{'' if visible else '<visibility>0</visibility>'}
<styleUrl>#{style}</styleUrl><description><![CDATA[{desc}]]></description>
<Point><altitudeMode>{mode}</altitudeMode><coordinates>{lon:.7f},{lat:.7f},{alt:.2f}</coordinates></Point></Placemark>"""


def kml_line(name, lls, alts, style, extrude=False, mode="relativeToGround", desc="", visible=True):
    return f"""<Placemark><name>{escape(name)}</name>{'' if visible else '<visibility>0</visibility>'}
<styleUrl>#{style}</styleUrl><description><![CDATA[{desc}]]></description>
<LineString><extrude>{1 if extrude else 0}</extrude><tessellate>1</tessellate>
<altitudeMode>{mode}</altitudeMode><coordinates>{coords(lls, alts)}</coordinates></LineString></Placemark>"""


def kml_polygon(name, lls, style, desc=""):
    ring = list(lls) + [lls[0]]
    c = " ".join(f"{lo:.7f},{la:.7f},0" for la, lo in ring)
    return f"""<Placemark><name>{escape(name)}</name><styleUrl>#{style}</styleUrl>
<description><![CDATA[{desc}]]></description>
<Polygon><tessellate>1</tessellate><altitudeMode>clampToGround</altitudeMode>
<outerBoundaryIs><LinearRing><coordinates>{c}</coordinates></LinearRing></outerBoundaryIs></Polygon></Placemark>"""


def styles():
    s = []
    for key, (line, poly, _) in PHASE_STYLE.items():
        s.append(f'<Style id="ph_{key}"><LineStyle><color>{line}</color><width>4</width></LineStyle>'
                 f'<PolyStyle><color>{poly}</color><outline>0</outline></PolyStyle></Style>')
    icon = "http://maps.google.com/mapfiles/kml/"
    s += [
        f'<Style id="pad"><IconStyle><scale>1.3</scale><Icon><href>{icon}shapes/triangle.png</href></Icon><color>ff00ff00</color></IconStyle><LabelStyle><scale>1</scale></LabelStyle></Style>',
        f'<Style id="land"><IconStyle><scale>1.3</scale><Icon><href>{icon}shapes/target.png</href></Icon><color>ff0000ff</color></IconStyle></Style>',
        f'<Style id="event"><IconStyle><scale>0.8</scale><Icon><href>{icon}shapes/placemark_circle.png</href></Icon></IconStyle><LabelStyle><scale>0.8</scale></LabelStyle></Style>',
        f'<Style id="mc"><IconStyle><scale>0.45</scale><color>ccffffff</color><Icon><href>{icon}shapes/shaded_dot.png</href></Icon></IconStyle><LabelStyle><scale>0</scale></LabelStyle></Style>',
        f'<Style id="sweep"><IconStyle><scale>0.7</scale><color>ff00ffff</color><Icon><href>{icon}shapes/open-diamond.png</href></Icon></IconStyle><LabelStyle><scale>0.7</scale></LabelStyle></Style>',
        '<Style id="walk"><LineStyle><color>ff0000ff</color><width>3</width></LineStyle></Style>',
        '<Style id="sweepline"><LineStyle><color>9900ffff</color><width>2</width></LineStyle></Style>',
        '<Style id="ell1"><LineStyle><color>ff00ff00</color><width>2</width></LineStyle><PolyStyle><color>3300ff00</color></PolyStyle></Style>',
        '<Style id="ell2"><LineStyle><color>ff00ffff</color><width>2</width></LineStyle><PolyStyle><color>2200ffff</color></PolyStyle></Style>',
        '<Style id="ell3"><LineStyle><color>ff0000ff</color><width>2</width></LineStyle><PolyStyle><color>110000ff</color></PolyStyle></Style>',
        f'<Style id="rocket"><IconStyle><scale>1.1</scale><Icon><href>{icon}shapes/airports.png</href></Icon></IconStyle></Style>',
    ]
    return "\n".join(s)


def summary_html(name, traj, dist, brg):
    s, c = traj["summary"], traj["conditions"]
    rows = [
        ("Apogee", fmt_dist(s["max_alt"])),
        ("Max velocity", f"{s['max_v']:.1f} m/s (Mach {max(traj['mach']):.2f})"),
        ("Time to apogee", f"{s['apogee_t']:.1f} s"),
        ("Flight time", f"{s['flight_t']:.1f} s"),
        ("Off-rod velocity", f"{s['rod_v']:.1f} m/s"),
        ("Landing velocity", f"{s['ground_v']:.1f} m/s"),
        ("Drift to landing", f"{fmt_dist(dist)} toward {brg:.0f}° {compass(brg)}"),
        ("Wind", f"{c['wind_speed']:.1f} m/s from {c['wind_dir_deg']:.0f}° {compass(c['wind_dir_deg'])}, "
                 f"turbulence {c['turbulence'] * 100:.0f}%"),
        ("Rod", f"{c['rod_length']:.2f} m, {c['rod_angle_deg']:.1f}° tilt"),
    ]
    return f"<b>{escape(name)}</b><table>" + "".join(
        f"<tr><td><b>{k}</b></td><td>{v}</td></tr>" for k, v in rows) + "</table>"


def sim_folder(name, traj, lls, t0, mc=None, sweep=None, pad=None, open_=False):
    alts = traj["alt"]
    lat0, lon0 = pad
    parts = []
    le, ln = landing(traj)
    dist, brg = math.hypot(le, ln), bearing_deg(le, ln)
    land_ll = lls[-1]

    # flight path, coloured by phase
    ph = phases(traj)
    path = []
    for key, (a, b) in ph.items():
        if b <= a:
            continue
        sl = slice(a, b + 1)
        path.append(kml_line(PHASE_STYLE[key][2], lls[sl], alts[sl], f"ph_{key}", extrude=True))
    parts.append(f"<Folder><name>Flight path</name>{''.join(path)}</Folder>")

    # animated track
    whens, cs = [], []
    for t, (la, lo), a in zip(traj["t"], lls, alts):
        whens.append(f"<when>{(t0 + dt.timedelta(seconds=t)).strftime('%Y-%m-%dT%H:%M:%S.%f')[:-3]}Z</when>")
        cs.append(f"<gx:coord>{lo:.7f} {la:.7f} {max(a, 0):.2f}</gx:coord>")
    parts.append(f"""<Placemark><name>Rocket (press play on time slider)</name><styleUrl>#rocket</styleUrl>
<gx:Track><altitudeMode>relativeToGround</altitudeMode>{''.join(whens)}{''.join(cs)}</gx:Track></Placemark>""")

    # events
    ev = []
    for e in traj["events"]:
        if e["type"] not in EVENT_LABELS or e["type"] == "GROUND_HIT":
            continue
        i = idx_at(traj, e["t"])
        la, lo = lls[i]
        label = f"{EVENT_LABELS[e['type']]} T+{e['t']:.1f}s"
        desc = f"Altitude {alts[i]:.0f} m AGL<br>Velocity {traj['v'][i]:.1f} m/s"
        ev.append(kml_point(label, la, lo, alts[i], "event", desc))
    parts.append(f"<Folder><name>Events</name>{''.join(ev)}</Folder>")

    # landing + recovery walk
    parts.append(kml_point(f"Landing: {dist:.0f} m {compass(brg)}", land_ll[0], land_ll[1], 0, "land",
                           summary_html(name, traj, dist, brg), mode="clampToGround"))
    parts.append(kml_line(f"Recovery walk {fmt_dist(dist)}", [pad, land_ll], [0, 0], "walk",
                          mode="clampToGround"))

    # monte carlo
    if mc:
        pts = [(m["e"], m["n"]) for m in mc]
        dots = "".join(kml_point(
            f"MC {i}", *enu_to_ll(lat0, lon0, m["e"], m["n"]), 0, "mc",
            f"wind {m['wind']:.1f} m/s from {m['dir']:.0f}°<br>apogee {m['apogee']:.0f} m<br>"
            f"drift {math.hypot(m['e'], m['n']):.0f} m", mode="clampToGround") for i, m in enumerate(mc))
        ells = []
        for k, pct in ((1, 39.3), (2, 86.5), (3, 98.9)):
            ell, mu, axes = sigma_ellipse(pts, k)
            ll = [enu_to_ll(lat0, lon0, e, n) for e, n in ell]
            ells.append(kml_polygon(f"{k}σ landing zone (~{pct}% of landings)", ll, f"ell{k}",
                                    f"Semi-axes {axes[1]:.0f} m × {axes[0]:.0f} m"))
        d = np.hypot(*np.array(pts).T)
        stats = (f"{len(mc)} runs. Drift: median {np.median(d):.0f} m, 90th pct {np.percentile(d, 90):.0f} m, "
                 f"max {d.max():.0f} m. Apogee {min(m['apogee'] for m in mc):.0f}–{max(m['apogee'] for m in mc):.0f} m.")
        parts.append(f"<Folder><name>Monte Carlo dispersion</name><description>{escape(stats)}</description>"
                     f"{''.join(ells)}<Folder><name>Landing points</name>{dots}</Folder></Folder>")

    # wind sweep
    if sweep:
        sp = []
        lls_s = [pad]
        for w in sweep:
            la, lo = enu_to_ll(lat0, lon0, w["e"], w["n"])
            lls_s.append((la, lo))
            sp.append(kml_point(f"{w['wind']:.0f} m/s → {math.hypot(w['e'], w['n']):.0f} m", la, lo, 0, "sweep",
                                f"Wind {w['wind']:.1f} m/s ({w['wind'] * 2.237:.0f} mph): lands "
                                f"{fmt_dist(math.hypot(w['e'], w['n']))}", mode="clampToGround"))
        sp.append(kml_line("Drift line", lls_s, [0] * len(lls_s), "sweepline", mode="clampToGround"))
        parts.append(f"<Folder><name>Wind sweep (same direction)</name><visibility>0</visibility>{''.join(sp)}</Folder>")

    return (f"<Folder><name>{escape(name)}</name><open>{1 if open_ else 0}</open>"
            f"<description><![CDATA[{summary_html(name, traj, dist, brg)}]]></description>"
            f"{''.join(parts)}</Folder>")


# ---------------------------------------------------------------- main
def serve(path, data=None):
    """Serve a page on localhost and open it; CesiumJS's workers don't run from file://.

    `path` is the page to open; its folder is the web root. `data`, if given, is also served at /data.json.
    """
    import functools
    import http.server
    import webbrowser
    path = os.path.abspath(path)
    data = data and os.path.abspath(data)

    class Handler(http.server.SimpleHTTPRequestHandler):
        # Module workers (MapLibre's .mjs) need a JavaScript MIME type; older Pythons don't map .mjs.
        extensions_map = {**http.server.SimpleHTTPRequestHandler.extensions_map, ".mjs": "text/javascript"}

        def translate_path(self, p):
            if data and p.split("?", 1)[0] == "/data.json":
                return data
            return super().translate_path(p)

        def log_message(self, *args):
            pass

    handler = functools.partial(Handler, directory=os.path.dirname(path))
    with http.server.ThreadingHTTPServer(("127.0.0.1", 0), handler) as srv:
        page = os.path.basename(path)
        url = f"http://127.0.0.1:{srv.server_address[1]}/{'' if page == 'index.html' else page}"
        print(f"serving {url}  (Ctrl-C to stop)", file=sys.stderr)
        webbrowser.open(url)
        try:
            srv.serve_forever()
        except KeyboardInterrupt:
            pass


def serve_app(data):
    """Serve the React app (web/dist) with `data` (.rocketviz.json) as its flight data."""
    index = os.path.join(HERE, "web", "dist", "index.html")
    if not os.path.exists(index):
        sys.exit("web/dist is missing; build the app first:  cd web && npm install && npm run build")
    serve(index, data)


def sig(v, digits=6):
    """Round to significant digits (keeps the app JSON small); None passes through."""
    return None if v is None or v != v else float(f"{v:.{digits}g}")


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("ork", help=".ork to simulate, or an existing viewer .html / app .rocketviz.json to just serve")
    ap.add_argument("--lat", type=float, help="override launch latitude")
    ap.add_argument("--lon", type=float, help="override launch longitude")
    ap.add_argument("--wind", type=float, help="override mean wind speed (m/s)")
    ap.add_argument("--wind-from", type=float, help="override wind direction, degrees it blows FROM")
    ap.add_argument("--mc", type=int, default=150, help="Monte Carlo runs per simulation (0 = off)")
    ap.add_argument("--sigma-wind", type=float, default=None, help="1σ wind speed (default 30%% of mean + 0.3)")
    ap.add_argument("--sigma-dir", type=float, default=20.0, help="1σ wind direction, degrees")
    ap.add_argument("--sigma-rod", type=float, default=1.5, help="1σ rod tilt error, degrees")
    ap.add_argument("--sweep-max", type=int, default=8, help="wind sweep up to this many m/s")
    ap.add_argument("--sims", help="comma list of simulation indices (default all)")
    ap.add_argument("--out", help="output .kml")
    ap.add_argument("--json", help="also write trajectories as JSON")
    ap.add_argument("--html", help="also write a self-contained 3D globe viewer (CesiumJS)")
    ap.add_argument("--app", nargs="?", const="", metavar="JSON",
                    help="also write flight data for the React app (default <rocket>.rocketviz.json)")
    ap.add_argument("--serve", action="store_true",
                    help="open the result in a browser over localhost: the React app with --app, else the --html viewer")
    ap.add_argument("--seed", type=int, default=42)
    a = ap.parse_args()

    if a.ork.lower().endswith(".html"):
        serve(a.ork)
        return
    if a.ork.lower().endswith(".json"):
        serve_app(a.ork)
        return
    if a.app == "":
        a.app = a.ork.rsplit(".", 1)[0] + ".rocketviz.json"
    if a.serve and not a.app and not a.html:
        a.html = a.ork.rsplit(".", 1)[0] + "_globe.html"

    rng = random.Random(a.seed)
    doc = orsim.load(a.ork)
    info = orsim.describe(doc)
    sims = list(doc.getSimulations())
    pick = [int(i) for i in a.sims.split(",")] if a.sims else range(len(sims))
    t0 = dt.datetime(2026, 1, 1, 12, 0, 0)

    folders, js = [], {"rocket": info["rocket"], "sims": []}
    pad = None
    for idx in pick:
        sim = sims[idx]
        meta = info["sims"][idx]
        name = f"{meta['name']} — {meta['config']}"
        base = orsim.run(sim, wind_speed=a.wind, wind_dir_deg=a.wind_from, lat=a.lat, lon=a.lon, seed=a.seed,
                         all_series=bool(a.app))
        c = base["conditions"]
        # Configs can have different launch sites; the file's pad is the first one's (each keeps its own in conditions).
        pad = pad or (c["lat"], c["lon"])
        lls = georef(base)
        print(f"[{name}] apogee {base['summary']['max_alt']:.0f} m, lands "
              f"{math.hypot(*landing(base)):.0f} m {compass(bearing_deg(*landing(base)))}", file=sys.stderr)

        mc = []
        sw = a.sigma_wind if a.sigma_wind is not None else 0.3 * c["wind_speed"] + 0.3
        for i in range(a.mc):
            ws = max(0.0, rng.gauss(c["wind_speed"], sw))
            wd = (rng.gauss(c["wind_dir_deg"], a.sigma_dir)) % 360
            tilt = abs(rng.gauss(0, a.sigma_rod))
            r = orsim.run(sim, wind_speed=ws, wind_dir_deg=wd, lat=a.lat, lon=a.lon,
                          rod_angle_deg=c["rod_angle_deg"] + tilt, rod_dir_deg=rng.uniform(0, 360),
                          seed=rng.randrange(1 << 30), timestep=0.05)
            e, n = landing(r)
            mc.append({"e": e, "n": n, "wind": ws, "dir": wd, "apogee": r["summary"]["max_alt"]})
        if a.mc:
            d = [math.hypot(m["e"], m["n"]) for m in mc]
            print(f"   MC x{a.mc}: median drift {np.median(d):.0f} m, max {max(d):.0f} m", file=sys.stderr)

        sweep = []
        for w in range(0, a.sweep_max + 1):
            r = orsim.run(sim, wind_speed=float(w), wind_dir_deg=c["wind_dir_deg"], turbulence=0.0,
                          lat=a.lat, lon=a.lon, seed=1)
            e, n = landing(r)
            sweep.append({"wind": float(w), "e": e, "n": n, "apogee": r["summary"]["max_alt"]})

        folders.append(sim_folder(name, base, lls, t0, mc, sweep, (c["lat"], c["lon"]), open_=(idx == pick[0])))
        js["sims"].append({
            "name": name, "conditions": c, "summary": base["summary"], "events": base["events"],
            "t": base["t"], "lat": [p[0] for p in lls], "lon": [p[1] for p in lls], "alt": base["alt"],
            "v": base["v"], "e": base["x"], "n": base["y"], "mc": mc, "sweep": sweep,
            "config": meta["config"], "series": base.get("series"),
        })

    lat0, lon0 = pad
    pad_desc = (f"Launch site {lat0:.5f}, {lon0:.5f}<br>Site altitude used by OpenRocket "
                f"(air density): {base['conditions']['site_alt']:.0f} m MSL")
    doc_kml = f"""<?xml version="1.0" encoding="UTF-8"?>
<kml xmlns="http://www.opengis.net/kml/2.2" xmlns:gx="http://www.google.com/kml/ext/2.2">
<Document><name>{escape(info['rocket'])} — OpenRocket flight</name><open>1</open>
<LookAt><longitude>{lon0}</longitude><latitude>{lat0}</latitude><altitude>0</altitude>
<heading>0</heading><tilt>60</tilt><range>1500</range><altitudeMode>relativeToGround</altitudeMode></LookAt>
{styles()}
{kml_point('Launch pad', lat0, lon0, 0, 'pad', pad_desc, mode='clampToGround')}
{''.join(folders)}
</Document></kml>"""
    out = a.out or a.ork.rsplit(".", 1)[0] + ".kml"
    open(out, "w").write(doc_kml)
    print(f"wrote {out}", file=sys.stderr)
    js["pad"] = {"lat": lat0, "lon": lon0}
    if a.html:
        tpl = open(os.path.join(os.path.dirname(os.path.abspath(__file__)), "viewer_template.html")).read()
        slim = json.loads(json.dumps(js))
        for sm in slim["sims"]:
            for k in ("t", "alt", "v", "e", "n"):
                sm[k] = [round(x, 2) for x in sm[k]]
            sm.pop("lat"); sm.pop("lon")
            for m in sm["mc"] + sm["sweep"]:
                for k in m:
                    m[k] = round(m[k], 2)
        html = tpl.replace("__TITLE__", escape(info["rocket"])).replace(
            "__DATA__", json.dumps(slim, separators=(",", ":")).replace("</", "<\\/"))
        open(a.html, "w").write(html)
        print(f"wrote {a.html}", file=sys.stderr)
    if a.json:
        json.dump({**js, "sims": [{k: v for k, v in sm.items() if k != "series"} for sm in js["sims"]]},
                  open(a.json, "w"))
        print(f"wrote {a.json}", file=sys.stderr)
    if a.app:
        app = {"version": 1, "rocket": js["rocket"], "pad": js["pad"],
               "site_alt": base["conditions"]["site_alt"], "sims": []}
        for sm in js["sims"]:
            app["sims"].append({
                "name": sm["name"], "config": sm["config"], "conditions": sm["conditions"],
                "summary": sm["summary"], "events": sm["events"],
                "t": [sig(x) for x in sm["t"]], "e": [sig(x) for x in sm["e"]],
                "n": [sig(x) for x in sm["n"]], "alt": [sig(x) for x in sm["alt"]],
                "series": [{**se, "data": [sig(x) for x in se["data"]]} for se in sm["series"]
                           if se["key"] != "time" and any(x is not None for x in se["data"])],
                "mc": [{k: sig(v) for k, v in m.items()} for m in sm["mc"]],
                "sweep": [{k: sig(v) for k, v in w.items()} for w in sm["sweep"]],
            })
        with open(a.app, "w") as f:
            json.dump(app, f, separators=(",", ":"), allow_nan=False)
        print(f"wrote {a.app}", file=sys.stderr)
    if a.serve:
        serve_app(a.app) if a.app else serve(a.html)


if __name__ == "__main__":
    main()
