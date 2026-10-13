package net.rocketviz.engine.openrocket;

import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.DynamicTest;
import org.junit.jupiter.api.TestFactory;

/**
 * The bridge must reproduce reference results computed independently (Python/JPype driving the same OpenRocket with
 * the same seeding rules): src/test/resources/golden/fizz.json, generated on an Apple Silicon Mac.
 *
 * <p>On that platform the match is exact. Elsewhere, Java's {@code Math} functions may differ in the last bit between
 * CPU architectures, and OpenRocket's adaptive time step turns that into slightly different sampling, so results are
 * compared within physical tolerances instead (landing 5 cm, apogee 1 cm, event times one time step). Measured on
 * x86 Linux and Windows CI: landings within 8 mm. Getting the seeding or an override wrong moves the landing by about
 * a metre, so the check still catches the bugs it exists for.
 */
class GoldenParityTest {
    /** Exact on the platform that generated the golden file; {@code -Dgolden.strict=false} forces tolerances. */
    private static final boolean STRICT = Boolean.parseBoolean(System.getProperty("golden.strict",
        String.valueOf(System.getProperty("os.name").startsWith("Mac") && System.getProperty("os.arch").equals("aarch64"))));
    private static final double EXACT = 1e-9;
    private static final double LANDING_M = 0.05;
    private static final double ALTITUDE_M = 0.01;
    /**
     * Event times can shift by one simulation step (up to 0.05 s): apogee, for instance, is detected at whichever step
     * crosses it, and near apogee vertical speed is ~0. Apogee altitude still agrees to the millimetre.
     */
    private static final double TIME_S = 0.06;
    private static final double SPEED_MS = 0.01;

    @TestFactory
    List<DynamicTest> matchesGolden() throws Exception {
        JsonObject golden = Fixtures.golden();
        OpenRocketEngine engine = new OpenRocketEngine();
        JsonObject design = engine.load(Fixtures.fizz().toString());
        String designId = design.get("design_id").getAsString();
        assertEquals(golden.get("rocket").getAsString(), design.get("name").getAsString());
        System.err.println("golden parity mode: " + (STRICT ? "exact" : "physical tolerances") + " ("
            + System.getProperty("os.name") + " " + System.getProperty("os.arch") + ")");

        List<DynamicTest> tests = new ArrayList<>();
        for (JsonElement ce : golden.getAsJsonArray("configs")) {
            JsonObject cfg = ce.getAsJsonObject();
            String configId = cfg.get("id").getAsString();
            for (JsonElement ke : cfg.getAsJsonArray("cases")) {
                JsonObject c = ke.getAsJsonObject();
                String name = cfg.get("motor").getAsString() + " " + c.get("id").getAsString();
                tests.add(DynamicTest.dynamicTest(name, () -> check(engine, designId, configId, c, name)));
            }
        }
        return tests;
    }

    private static void check(OpenRocketEngine engine, String designId, String configId, JsonObject c, String name)
            throws Exception {
        JsonObject r = engine.run(designId, configId, c.getAsJsonObject("overrides"), Output.TRACK);
        Diffs d = new Diffs(name);
        JsonArray t = r.getAsJsonArray("t");
        if (STRICT) assertEquals(c.get("samples").getAsInt(), t.size(), "sample count");
        d.check("end time", c.get("end_t"), t.get(t.size() - 1), TIME_S);
        JsonObject gl = c.getAsJsonObject("landing"), rl = r.getAsJsonObject("landing");
        d.check("landing east", gl.get("e"), rl.get("e"), LANDING_M);
        d.check("landing north", gl.get("n"), rl.get("n"), LANDING_M);
        JsonObject gs = c.getAsJsonObject("summary"), rs = r.getAsJsonObject("summary");
        for (String k : gs.keySet()) {
            double tol = k.endsWith("_time") ? TIME_S : k.endsWith("_alt") ? ALTITUDE_M : SPEED_MS;
            d.check("summary." + k, gs.get(k), rs.get(k), tol);
        }
        JsonArray ge = c.getAsJsonArray("events"), re = r.getAsJsonArray("events");
        assertEquals(ge.size(), re.size(), "event count");
        for (int i = 0; i < ge.size(); i++) {
            JsonObject a = ge.get(i).getAsJsonObject(), b = re.get(i).getAsJsonObject();
            assertEquals(a.get("type").getAsString(), b.get("type").getAsString(), "event " + i);
            d.check("event " + i + " (" + a.get("type").getAsString() + ") time", a.get("t"), b.get("t"), TIME_S);
        }
        d.report();
    }

    /** Collects every difference so a failure (or a CI log) shows all of them, not just the first. */
    private static final class Diffs {
        private final String name;
        private final List<String> failures = new ArrayList<>();
        private double worstLanding;

        Diffs(String name) {
            this.name = name;
        }

        void check(String what, JsonElement expected, JsonElement actual, double tolerance) {
            double e = expected.getAsDouble(), a = actual.getAsDouble(), diff = Math.abs(e - a);
            double tol = STRICT ? EXACT * Math.max(1, Math.abs(e)) : tolerance;
            if (what.startsWith("landing")) worstLanding = Math.max(worstLanding, diff);
            if (diff > tol) failures.add(String.format("%s: expected %.9f, got %.9f (diff %.3g, tolerance %.3g)", what, e, a, diff, tol));
        }

        void report() {
            System.err.printf("golden %-28s landing diff %.3g m%n", name, worstLanding);
            assertTrue(failures.isEmpty(), name + ":\n  " + String.join("\n  ", failures));
        }
    }
}
