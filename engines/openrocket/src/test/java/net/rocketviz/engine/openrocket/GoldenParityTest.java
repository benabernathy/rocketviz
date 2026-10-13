package net.rocketviz.engine.openrocket;

import static org.junit.jupiter.api.Assertions.assertEquals;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.List;
import org.junit.jupiter.api.DynamicTest;
import org.junit.jupiter.api.TestFactory;

/**
 * The bridge must reproduce reference results computed independently (Python/JPype driving the same OpenRocket with
 * the same seeding rules): src/test/resources/golden/fizz.json.
 */
class GoldenParityTest {
    private static final double TOL = 1e-9;

    @TestFactory
    List<DynamicTest> matchesGolden() throws Exception {
        JsonObject golden = Fixtures.golden();
        OpenRocketEngine engine = new OpenRocketEngine();
        JsonObject design = engine.load(Fixtures.fizz().toString());
        String designId = design.get("design_id").getAsString();
        assertEquals(golden.get("rocket").getAsString(), design.get("name").getAsString());

        List<DynamicTest> tests = new ArrayList<>();
        for (JsonElement ce : golden.getAsJsonArray("configs")) {
            JsonObject cfg = ce.getAsJsonObject();
            String configId = cfg.get("id").getAsString();
            for (JsonElement ke : cfg.getAsJsonArray("cases")) {
                JsonObject c = ke.getAsJsonObject();
                String name = cfg.get("motor").getAsString() + " " + c.get("id").getAsString();
                tests.add(DynamicTest.dynamicTest(name, () -> {
                    JsonObject r = engine.run(designId, configId, c.getAsJsonObject("overrides"), Output.TRACK);
                    assertEquals(c.get("samples").getAsInt(), r.getAsJsonArray("t").size(), "sample count");
                    JsonArray t = r.getAsJsonArray("t");
                    near(c.get("end_t").getAsDouble(), t.get(t.size() - 1).getAsDouble(), "end time");
                    near(c.getAsJsonObject("landing").get("e").getAsDouble(), r.getAsJsonObject("landing").get("e").getAsDouble(), "landing east");
                    near(c.getAsJsonObject("landing").get("n").getAsDouble(), r.getAsJsonObject("landing").get("n").getAsDouble(), "landing north");
                    JsonObject gs = c.getAsJsonObject("summary"), rs = r.getAsJsonObject("summary");
                    for (String k : gs.keySet()) near(gs.get(k).getAsDouble(), rs.get(k).getAsDouble(), "summary." + k);
                    JsonArray ge = c.getAsJsonArray("events"), re = r.getAsJsonArray("events");
                    assertEquals(ge.size(), re.size(), "event count");
                    for (int i = 0; i < ge.size(); i++) {
                        JsonObject a = ge.get(i).getAsJsonObject(), b = re.get(i).getAsJsonObject();
                        assertEquals(a.get("type").getAsString(), b.get("type").getAsString(), "event " + i);
                        near(a.get("t").getAsDouble(), b.get("t").getAsDouble(), "event " + i + " time");
                    }
                }));
            }
        }
        return tests;
    }

    private static void near(double expected, double actual, String what) {
        assertEquals(expected, actual, TOL * Math.max(1, Math.abs(expected)), what);
    }
}
