package net.rocketviz.engine.openrocket;

import static net.rocketviz.engine.openrocket.RpcClient.obj;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertFalse;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import java.io.BufferedReader;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.PipedInputStream;
import java.io.PipedOutputStream;
import java.nio.charset.StandardCharsets;
import java.util.HashSet;
import java.util.Set;
import java.util.concurrent.atomic.AtomicInteger;
import net.rocketviz.engine.openrocket.rpc.JsonRpcServer;
import org.junit.jupiter.api.AfterEach;
import org.junit.jupiter.api.BeforeEach;
import org.junit.jupiter.api.Test;

/** The JSON-RPC surface, against an in-process server. */
class ProtocolTest {
    private RpcClient client;
    private Thread serverThread;
    private final AtomicInteger exitCode = new AtomicInteger(-1);
    private String designId;

    @BeforeEach
    void start() throws Exception {
        PipedOutputStream toServer = new PipedOutputStream();
        PipedInputStream serverIn = new PipedInputStream(toServer, 1 << 20);
        PipedOutputStream serverOut = new PipedOutputStream();
        PipedInputStream fromServer = new PipedInputStream(serverOut, 1 << 22);
        JsonRpcServer server = new JsonRpcServer(
            new InputStreamReader(serverIn, StandardCharsets.UTF_8),
            new OutputStreamWriter(serverOut, StandardCharsets.UTF_8),
            Main.handlers(new OpenRocketEngine()));
        serverThread = new Thread(() -> {
            try {
                exitCode.set(server.serve());
                serverOut.close();
            } catch (Exception e) {
                throw new RuntimeException(e);
            }
        });
        serverThread.start();
        client = new RpcClient(new BufferedReader(new InputStreamReader(fromServer, StandardCharsets.UTF_8)),
            new OutputStreamWriter(toServer, StandardCharsets.UTF_8));

        JsonObject init = client.call("initialize", obj("protocol_version", 1)).getAsJsonObject("result");
        assertEquals("openrocket", init.getAsJsonObject("engine").get("id").getAsString());
        JsonObject load = client.call("load", obj("path", Fixtures.fizz().toString())).getAsJsonObject("result");
        designId = load.get("design_id").getAsString();
        assertEquals(2, load.getAsJsonArray("configs").size());
    }

    @AfterEach
    void stop() throws Exception {
        if (serverThread.isAlive()) {
            client.call("shutdown", null);
            client.notifyEngine("exit", null);
            serverThread.join(30_000);
        }
    }

    @Test
    void initializeDescribesParameters() throws Exception {
        JsonObject init = client.call("initialize", obj("protocol_version", 1)).getAsJsonObject("result");
        Set<String> keys = new HashSet<>();
        init.getAsJsonArray("parameters").forEach(p -> keys.add(p.getAsJsonObject().get("key").getAsString()));
        assertTrue(keys.containsAll(Set.of("wind_speed", "wind_direction", "turbulence_intensity", "rod_angle",
            "rod_direction", "launch_latitude", "launch_longitude", "seed", "time_step")), keys.toString());
        assertTrue(init.getAsJsonObject("capabilities").get("parallel").getAsBoolean());
    }

    @Test
    void rejectsUnsupportedProtocolVersion() throws Exception {
        assertError(client.call("initialize", obj("protocol_version", 99)), -32010);
    }

    @Test
    void loadReportsConfigsWithDefaults() throws Exception {
        JsonObject load = client.call("load", obj("path", Fixtures.fizz().toString())).getAsJsonObject("result");
        JsonObject c = load.getAsJsonArray("configs").get(1).getAsJsonObject();
        assertEquals("1", c.get("id").getAsString());
        assertEquals("[C6-7]", c.get("motor").getAsString());
        assertEquals(2.0, c.getAsJsonObject("defaults").get("wind_speed").getAsDouble(), 1e-9);
        assertEquals(90.0, c.getAsJsonObject("defaults").get("wind_direction").getAsDouble(), 1e-9);
    }

    @Test
    void runFullReturnsAlignedSeries() throws Exception {
        JsonObject r = client.call("run", obj("design_id", designId, "config_id", "0")).getAsJsonObject("result");
        int n = r.getAsJsonArray("t").size();
        assertTrue(n > 100);
        Set<String> keys = new HashSet<>();
        for (JsonElement s : r.getAsJsonArray("series")) {
            JsonObject o = s.getAsJsonObject();
            keys.add(o.get("key").getAsString());
            assertEquals(n, o.getAsJsonArray("data").size(), o.get("key").getAsString() + " length");
        }
        assertTrue(keys.containsAll(Set.of("altitude", "total_velocity", "vertical_velocity", "total_acceleration",
            "mach_number", "thrust", "drag_force", "mass", "angle_of_attack")), keys.toString());
        assertFalse(keys.contains("time"));
    }

    @Test
    void runLandingIsCompact() throws Exception {
        JsonObject r = client.call("run", obj("design_id", designId, "config_id", "0", "output", "landing")).getAsJsonObject("result");
        assertEquals(Set.of("summary", "landing"), r.keySet());
    }

    @Test
    void errors() throws Exception {
        assertError(client.call("nope", null), -32601);
        assertError(client.call("run", obj("config_id", "0")), -32602);
        assertError(client.call("run", obj("design_id", "d999", "config_id", "0")), -32012);
        assertError(client.call("run", obj("design_id", designId, "config_id", "7")), -32012);
        assertError(client.call("run", obj("design_id", designId, "config_id", "0", "overrides", obj("warp_factor", 9))), -32014);
        assertError(client.call("run", obj("design_id", designId, "config_id", "0", "overrides", obj("wind_speed", -1))), -32014);
        assertError(client.call("load", obj("path", "/no/such/file.ork")), -32011);
        client.writeLine("{not json");
        assertEquals(-32700, client.next().getAsJsonObject("error").get("code").getAsInt());
    }

    @Test
    void batchMatchesSequentialAndReportsProgress() throws Exception {
        JsonArray cases = new JsonArray();
        for (int i = 0; i < 24; i++) {
            cases.add(obj("id", "mc-" + i, "overrides", obj("wind_speed", 1 + i * 0.2, "wind_direction", 60 + i * 5, "seed", 1000 + i)));
        }
        int id = client.send("run_batch", obj("design_id", designId, "config_id", "1", "output", "landing", "cases", cases));
        JsonObject result = client.await(id).getAsJsonObject("result");
        JsonArray results = result.getAsJsonArray("results");
        assertEquals(24, results.size());
        assertEquals(0, result.getAsJsonArray("failed").size());
        long progress = client.notifications.stream().filter(n -> n.get("method").getAsString().equals("progress")).count();
        assertEquals(24, progress);

        for (int i = 0; i < 24; i++) {
            JsonObject b = results.get(i).getAsJsonObject();
            assertEquals("mc-" + i, b.get("case_id").getAsString(), "results in case order");
            JsonObject seq = client.call("run", obj("design_id", designId, "config_id", "1", "output", "landing",
                "overrides", cases.get(i).getAsJsonObject().getAsJsonObject("overrides"))).getAsJsonObject("result");
            assertEquals(seq.getAsJsonObject("landing"), b.getAsJsonObject("result").getAsJsonObject("landing"), "case " + i);
        }
    }

    @Test
    void batchCanBeCancelled() throws Exception {
        JsonArray cases = new JsonArray();
        for (int i = 0; i < 400; i++) cases.add(obj("id", "c" + i, "overrides", obj("seed", i)));
        int id = client.send("run_batch", obj("design_id", designId, "config_id", "1", "output", "landing", "cases", cases));
        JsonObject first = client.next();
        assertEquals("progress", first.get("method").getAsString());
        client.notifyEngine("$/cancelRequest", obj("id", id));
        assertError(client.await(id), -32800);
        assertTrue(client.notifications.size() < 399, "batch stopped early: " + client.notifications.size());
    }

    @Test
    void cleanShutdown() throws Exception {
        assertTrue(client.call("shutdown", null).get("result").isJsonNull());
        assertError(client.call("run", obj("design_id", designId, "config_id", "0")), -32600);
        client.notifyEngine("exit", null);
        serverThread.join(30_000);
        assertEquals(0, exitCode.get());
    }

    private static void assertError(JsonObject response, int code) {
        assertTrue(response.has("error"), "expected error " + code + ", got " + response);
        assertEquals(code, response.getAsJsonObject("error").get("code").getAsInt(), response.toString());
    }
}
