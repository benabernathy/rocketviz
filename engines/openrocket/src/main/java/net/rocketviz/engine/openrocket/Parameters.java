package net.rocketviz.engine.openrocket;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import info.openrocket.core.models.wind.PinkNoiseWindModel;
import info.openrocket.core.models.wind.WindModelType;
import info.openrocket.core.simulation.SimulationOptions;
import java.lang.reflect.Field;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.function.ObjDoubleConsumer;
import java.util.function.ToDoubleFunction;
import net.rocketviz.engine.openrocket.rpc.RpcException;

/** The override parameters this engine accepts (docs/engine-protocol.md §5) and how they map onto OpenRocket. */
final class Parameters {
    private Parameters() {}

    /** Seed used when the host doesn't give one, so reopening a design reproduces the same flight. */
    static final int DEFAULT_SEED = 0;

    record Param(String key, String name, String unit, Double min, Double max, boolean circular,
                 ToDoubleFunction<SimulationOptions> get, ObjDoubleConsumer<SimulationOptions> set) {}

    /**
     * Overriding the wind uses OpenRocket's single-level ("average") wind model, as its own wind setters do. A design
     * set up with multi-level wind keeps it unless a wind parameter is overridden.
     */
    private static PinkNoiseWindModel averageWind(SimulationOptions o) {
        o.setWindModelType(WindModelType.AVERAGE);
        return o.getAverageWindModel();
    }

    private static double deg(double rad) {
        return Math.toDegrees(rad);
    }

    private static double rad(double deg) {
        return Math.toRadians(deg);
    }

    static final List<Param> ALL = List.of(
        new Param("wind_speed", "Wind speed", "m/s", 0.0, null, false,
            o -> o.getAverageWindModel().getAverage(), (o, v) -> averageWind(o).setAverage(v)),
        new Param("wind_direction", "Wind direction (from)", "deg", null, null, true,
            o -> deg(o.getAverageWindModel().getDirection()), (o, v) -> averageWind(o).setDirection(rad(v))),
        new Param("turbulence_intensity", "Turbulence intensity", "fraction", 0.0, 1.0, false,
            o -> o.getAverageWindModel().getTurbulenceIntensity(), (o, v) -> averageWind(o).setTurbulenceIntensity(v)),
        new Param("rod_angle", "Rod angle from vertical", "deg", 0.0, 90.0, false,
            o -> deg(o.getLaunchRodAngle()), (o, v) -> o.setLaunchRodAngle(rad(v))),
        // An explicit rod direction must win over OpenRocket's "launch into wind", which otherwise replaces it.
        new Param("rod_direction", "Rod direction", "deg", null, null, true,
            o -> deg(o.getLaunchRodDirection()), (o, v) -> { o.setLaunchIntoWind(false); o.setLaunchRodDirection(rad(v)); }),
        new Param("rod_length", "Rod length", "m", 0.0, null, false,
            SimulationOptions::getLaunchRodLength, SimulationOptions::setLaunchRodLength),
        new Param("launch_latitude", "Launch latitude", "deg", -90.0, 90.0, false,
            SimulationOptions::getLaunchLatitude, SimulationOptions::setLaunchLatitude),
        new Param("launch_longitude", "Launch longitude", "deg", -180.0, 180.0, false,
            SimulationOptions::getLaunchLongitude, SimulationOptions::setLaunchLongitude),
        new Param("launch_altitude", "Launch altitude (MSL)", "m", -500.0, 10000.0, false,
            SimulationOptions::getLaunchAltitude, SimulationOptions::setLaunchAltitude),
        new Param("seed", "Random seed", "", null, null, false,
            o -> o.getRandomSeed(), (o, v) -> seedWind(o, (int) v)),
        new Param("time_step", "Time step", "s", 1e-4, 1.0, false,
            SimulationOptions::getTimeStep, SimulationOptions::setTimeStep));

    private static final Map<String, Param> BY_KEY = new LinkedHashMap<>();

    static {
        ALL.forEach(p -> BY_KEY.put(p.key(), p));
    }

    /** The {@code parameters} list for {@code initialize}. */
    static JsonArray describe() {
        JsonArray out = new JsonArray();
        for (Param p : ALL) {
            JsonObject o = new JsonObject();
            o.addProperty("key", p.key());
            o.addProperty("name", p.name());
            o.addProperty("unit", p.unit());
            o.addProperty("kind", "standard");
            if (p.min() != null) o.addProperty("min", p.min());
            if (p.max() != null) o.addProperty("max", p.max());
            out.add(o);
        }
        return out;
    }

    /** Current values of every parameter, keyed like the protocol's {@code defaults}/{@code conditions}. */
    static JsonObject read(SimulationOptions o) {
        JsonObject out = new JsonObject();
        for (Param p : ALL) {
            double v = p.get().applyAsDouble(o);
            if (p.key().equals("seed")) out.addProperty("seed", (int) v);
            else out.addProperty(p.key(), v);
        }
        out.addProperty("launch_into_wind", o.getLaunchIntoWind());
        return out;
    }

    /**
     * Applies overrides to a simulation's options. The wind model is always re-seeded (with the override's seed or
     * {@link #DEFAULT_SEED}) because OpenRocket otherwise seeds turbulence randomly once per process.
     */
    static void apply(SimulationOptions o, JsonObject overrides) throws RpcException {
        Map<Param, Double> values = new LinkedHashMap<>();
        for (Map.Entry<String, JsonElement> e : overrides.entrySet()) {
            Param p = BY_KEY.get(e.getKey());
            if (p == null) {
                throw RpcException.withField(RpcException.BAD_OVERRIDE, "Unknown parameter: " + e.getKey(), "key", e.getKey());
            }
            JsonElement v = e.getValue();
            if (!v.isJsonPrimitive() || !v.getAsJsonPrimitive().isNumber()) {
                throw RpcException.withField(RpcException.BAD_OVERRIDE, p.key() + " must be a number", "key", p.key());
            }
            double d = v.getAsDouble();
            if (!Double.isFinite(d)) {
                throw RpcException.withField(RpcException.BAD_OVERRIDE, p.key() + " must be finite", "key", p.key());
            }
            if (p.circular()) {
                d = ((d % 360) + 360) % 360;
            } else if ((p.min() != null && d < p.min()) || (p.max() != null && d > p.max())) {
                throw RpcException.withField(RpcException.BAD_OVERRIDE,
                    p.key() + " out of range [" + p.min() + ", " + p.max() + "]: " + d, "key", p.key());
            }
            values.put(p, d);
        }
        // Seed first: it replaces the wind model, and the wind settings below must land on the new one.
        Param seed = BY_KEY.get("seed");
        seed.set().accept(o, values.containsKey(seed) ? values.remove(seed) : DEFAULT_SEED);
        values.forEach((p, v) -> p.set().accept(o, v));
    }

    private static final Field AVERAGE_WIND_MODEL;

    static {
        try {
            AVERAGE_WIND_MODEL = SimulationOptions.class.getDeclaredField("averageWindModel");
            AVERAGE_WIND_MODEL.setAccessible(true);
        } catch (NoSuchFieldException e) {
            throw new ExceptionInInitializerError("OpenRocket changed SimulationOptions; update Parameters.seedWind: " + e);
        }
    }

    /**
     * OpenRocket's turbulence seed is a final field of the wind model, set randomly when the design loads; the
     * options' random seed never reaches it. Swap in a wind model built with our seed, keeping its other settings.
     */
    private static void seedWind(SimulationOptions o, int seed) {
        PinkNoiseWindModel seeded = new PinkNoiseWindModel(seed);
        seeded.loadFrom(o.getAverageWindModel());
        try {
            AVERAGE_WIND_MODEL.set(o, seeded);
        } catch (IllegalAccessException e) {
            throw new IllegalStateException(e);
        }
        o.setRandomSeed(seed);
    }
}
