package net.rocketviz.engine.openrocket;

import com.google.gson.JsonArray;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonPrimitive;
import info.openrocket.core.document.Simulation;
import info.openrocket.core.simulation.FlightData;
import info.openrocket.core.simulation.FlightDataBranch;
import info.openrocket.core.simulation.FlightDataType;
import info.openrocket.core.simulation.FlightEvent;
import java.util.List;
import java.util.Locale;
import net.rocketviz.engine.openrocket.rpc.RpcException;

/** Detail level of a run result (docs/engine-protocol.md, {@code run}), and the encoder for each. */
public enum Output {
    LANDING, SUMMARY, TRACK, FULL;

    public static Output parse(String s) throws RpcException {
        if (s == null) return FULL;
        try {
            return valueOf(s.toUpperCase(Locale.ROOT));
        } catch (IllegalArgumentException e) {
            throw new RpcException(RpcException.INVALID_PARAMS, "output must be landing, summary, track or full: " + s);
        }
    }

    static JsonObject write(Simulation sim, Output level) {
        FlightData fd = sim.getSimulatedData();
        FlightDataBranch b = fd.getBranch(0);
        List<Double> t = b.get(FlightDataType.TYPE_TIME);
        List<Double> e = b.get(FlightDataType.TYPE_POSITION_X);
        List<Double> n = b.get(FlightDataType.TYPE_POSITION_Y);
        List<Double> alt = b.get(FlightDataType.TYPE_ALTITUDE);

        JsonObject out = new JsonObject();
        JsonObject summary = new JsonObject();
        summary.add("max_alt", num(fd.getMaxAltitude()));
        summary.add("max_speed", num(fd.getMaxVelocity()));
        summary.add("apogee_time", num(fd.getTimeToApogee()));
        summary.add("flight_time", num(fd.getFlightTime()));
        summary.add("landing_speed", num(fd.getGroundHitVelocity()));
        summary.add("rod_exit_speed", num(fd.getLaunchRodVelocity()));
        out.add("summary", summary);
        JsonObject landing = new JsonObject();
        landing.add("e", num(last(e)));
        landing.add("n", num(last(n)));
        out.add("landing", landing);
        if (level == LANDING) return out;

        JsonArray events = new JsonArray();
        for (FlightEvent ev : b.getEvents()) {
            JsonObject o = new JsonObject();
            o.addProperty("type", ev.getType().name());
            o.add("t", num(ev.getTime()));
            events.add(o);
        }
        out.add("events", events);
        out.add("conditions", Parameters.read(sim.getOptions()));
        if (level == SUMMARY) return out;

        out.add("t", array(t, 1));
        out.add("e", array(e, 1));
        out.add("n", array(n, 1));
        out.add("alt", array(alt, 1));
        if (level == TRACK) return out;

        out.add("series", series(b));
        return out;
    }

    /** Every recorded flight data type except time (it's {@code t}), SI units, angles in degrees. */
    private static JsonArray series(FlightDataBranch b) {
        JsonArray out = new JsonArray();
        for (FlightDataType type : b.getTypes()) {
            if (type == FlightDataType.TYPE_TIME) continue;
            List<Double> data = b.get(type);
            if (data == null || data.stream().noneMatch(v -> v != null && Double.isFinite(v))) continue;
            String unit = type.getUnitGroup().getSIUnit().getUnit().replace("​", "").strip();
            double scale = 1;
            if (unit.equals("rad") || unit.equals("rad/s")) {
                unit = unit.replace("rad", "°");
                scale = Math.toDegrees(1);
            }
            JsonObject s = new JsonObject();
            s.addProperty("key", key(type.getName()));
            s.addProperty("name", type.getName());
            s.addProperty("symbol", type.getSymbol());
            s.addProperty("unit", unit);
            s.addProperty("group", type.getGroup() != null ? type.getGroup().getName() : "Other");
            s.add("data", array(data, scale));
            out.add(s);
        }
        return out;
    }

    /** "Vertical velocity" -> "vertical_velocity". Matches the keys rocketviz.py wrote, so saved plots carry over. */
    static String key(String name) {
        return name.toLowerCase(Locale.ROOT).replaceAll("[^a-z0-9]+", "_").replaceAll("^_+|_+$", "");
    }

    private static JsonArray array(List<Double> values, double scale) {
        JsonArray a = new JsonArray(values.size());
        for (Double v : values) a.add(v == null ? JsonNull.INSTANCE : num(v * scale));
        return a;
    }

    /** JSON has no NaN or infinity; the protocol uses null for missing values. */
    private static com.google.gson.JsonElement num(double v) {
        return Double.isFinite(v) ? new JsonPrimitive(v) : JsonNull.INSTANCE;
    }

    private static double last(List<Double> values) {
        return values == null || values.isEmpty() || values.get(values.size() - 1) == null ? Double.NaN : values.get(values.size() - 1);
    }
}
