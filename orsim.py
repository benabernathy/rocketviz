"""Headless OpenRocket driver (OpenRocket 24.12, package info.openrocket.*).

Loads a .ork, runs its simulations (optionally with perturbed launch conditions),
and returns trajectories as plain Python dicts.
"""
import math
import os
import re
import jpype
import jpype.imports

HERE = os.path.dirname(os.path.abspath(__file__))
JAR = os.path.join(HERE, "OpenRocket-24.12.jar")

_started = False


def start():
    global _started
    if _started:
        return
    jpype.startJVM("-Djava.awt.headless=true", classpath=[JAR], convertStrings=True)
    # Silence OpenRocket's verbose logging
    from org.slf4j import LoggerFactory
    from ch.qos.logback.classic import Level
    LoggerFactory.getLogger("ROOT").setLevel(Level.ERROR)
    from info.openrocket.core.startup import Application
    from info.openrocket.swing.startup import GuiModule
    from info.openrocket.core.plugin import PluginModule
    from com.google.inject import Guice
    gui = GuiModule()
    injector = Guice.createInjector(gui, PluginModule())
    Application.setInjector(injector)
    gui.startLoader()
    _started = True


def load(path):
    start()
    from java.io import File
    from info.openrocket.core.file import GeneralRocketLoader
    return GeneralRocketLoader(File(path)).load()


FIELDS = {
    "t": "TYPE_TIME",
    "alt": "TYPE_ALTITUDE",
    "x": "TYPE_POSITION_X",      # east of pad, m
    "y": "TYPE_POSITION_Y",      # north of pad, m
    "lat": "TYPE_LATITUDE",      # radians
    "lon": "TYPE_LONGITUDE",     # radians
    "v": "TYPE_VELOCITY_TOTAL",
    "vz": "TYPE_VELOCITY_Z",
    "mach": "TYPE_MACH_NUMBER",
    "zenith": "TYPE_ORIENTATION_THETA",
    "azimuth": "TYPE_ORIENTATION_PHI",
    "wind": "TYPE_WIND_VELOCITY",
}


def _series(branch):
    """Every flight data type OpenRocket recorded, in SI units (angles converted to degrees)."""
    out = []
    for ft in branch.getTypes():
        name = str(ft.getName())
        unit = str(ft.getUnitGroup().getSIUnit().getUnit()).replace("\u200b", "").strip()
        scale = 1.0
        if unit in ("rad", "rad/s"):
            unit, scale = unit.replace("rad", "°"), math.degrees(1)
        g = ft.getGroup()
        vals = [float(v) * scale if v is not None and v == v else None for v in branch.get(ft)]
        out.append({"key": re.sub(r"[^a-z0-9]+", "_", name.lower()).strip("_"), "name": name,
                    "symbol": str(ft.getSymbol()), "unit": unit,
                    "group": str(g.getName()) if g else "Other", "data": vals})
    return out


def run(sim, wind_speed=None, wind_dir_deg=None, turbulence=None,
        rod_angle_deg=None, rod_dir_deg=None, seed=None, timestep=None,
        lat=None, lon=None, all_series=False):
    """Simulate a copy of `sim` with optional overrides. Returns trajectory dict.

    all_series=True also returns every recorded flight data type under "series" (for plotting).
    """
    from info.openrocket.core.simulation import FlightDataType
    s = sim.copy()
    o = s.getOptions()
    if wind_speed is not None:
        o.setWindSpeedAverage(float(wind_speed))
    if wind_dir_deg is not None:
        o.setWindDirection(math.radians(wind_dir_deg))
    if turbulence is not None:
        o.setWindTurbulenceIntensity(float(turbulence))
    if rod_angle_deg is not None:
        o.setLaunchRodAngle(math.radians(rod_angle_deg))
    if rod_dir_deg is not None:
        o.setLaunchRodDirection(math.radians(rod_dir_deg))
    if lat is not None:
        o.setLaunchLatitude(float(lat))
    if lon is not None:
        o.setLaunchLongitude(float(lon))
    if timestep is not None:
        o.setTimeStep(float(timestep))
    if seed is not None:
        o.setRandomSeed(int(seed))
    s.simulate()
    fd = s.getSimulatedData()
    b = fd.getBranch(0)
    out = {}
    for k, name in FIELDS.items():
        lst = b.get(getattr(FlightDataType, name))
        out[k] = [float(v) if v is not None else float("nan") for v in lst] if lst is not None else None
    if all_series:
        out["series"] = _series(b)
    out["events"] = []
    for e in b.getEvents():
        out["events"].append({"type": str(e.getType().name()), "t": float(e.getTime())})
    out["summary"] = {
        "max_alt": float(fd.getMaxAltitude()),
        "max_v": float(fd.getMaxVelocity()),
        "apogee_t": float(fd.getTimeToApogee()),
        "flight_t": float(fd.getFlightTime()),
        "ground_v": float(fd.getGroundHitVelocity()),
        "rod_v": float(fd.getLaunchRodVelocity()),
    }
    out["conditions"] = {
        "wind_speed": float(o.getWindSpeedAverage()),
        "wind_dir_deg": math.degrees(float(o.getWindDirection())),
        "turbulence": float(o.getWindTurbulenceIntensity()),
        "rod_angle_deg": math.degrees(float(o.getLaunchRodAngle())),
        "rod_dir_deg": math.degrees(float(o.getLaunchRodDirection())),
        "rod_length": float(o.getLaunchRodLength()),
        "launch_into_wind": bool(o.getLaunchIntoWind()),
        "lat": float(o.getLaunchLatitude()),
        "lon": float(o.getLaunchLongitude()),
        "site_alt": float(o.getLaunchAltitude()),
    }
    return out


def describe(doc):
    rocket = doc.getRocket()
    sims = []
    for i, sim in enumerate(doc.getSimulations()):
        cfg = rocket.getFlightConfiguration(sim.getFlightConfigurationId())
        sims.append({"index": i, "name": str(sim.getName()),
                     "config": str(cfg.getName()) if cfg else "?"})
    return {"rocket": str(rocket.getName()), "sims": sims}
