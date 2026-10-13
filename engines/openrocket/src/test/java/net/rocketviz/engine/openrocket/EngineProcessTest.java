package net.rocketviz.engine.openrocket;

import static net.rocketviz.engine.openrocket.RpcClient.obj;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;

import com.google.gson.JsonObject;
import java.io.BufferedReader;
import java.io.File;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.concurrent.TimeUnit;
import org.junit.jupiter.api.Test;

/** The engine as the host runs it: a separate JVM whose stdout carries nothing but protocol messages. */
class EngineProcessTest {
    @Test
    void speaksOnlyProtocolOnStdout() throws Exception {
        String java = Path.of(System.getProperty("java.home"), "bin", "java").toString();
        Path stderr = Files.createTempFile("engine-stderr", ".log");
        Process p = new ProcessBuilder(List.of(java, "-Djava.awt.headless=true", "-Xmx512m",
                "-cp", System.getProperty("java.class.path"), Main.class.getName()))
            .redirectError(stderr.toFile())
            .start();
        RpcClient c = new RpcClient(
            new BufferedReader(new InputStreamReader(p.getInputStream(), StandardCharsets.UTF_8)),
            new OutputStreamWriter(p.getOutputStream(), StandardCharsets.UTF_8));

        long t0 = System.nanoTime();
        JsonObject init = c.call("initialize", obj("protocol_version", 1));
        assertTrue(init.has("result"), init.toString());
        JsonObject load = c.call("load", obj("path", Fixtures.fizz().toString())).getAsJsonObject("result");
        long readyMs = TimeUnit.NANOSECONDS.toMillis(System.nanoTime() - t0);
        String designId = load.get("design_id").getAsString();
        JsonObject run = c.call("run", obj("design_id", designId, "config_id", "0")).getAsJsonObject("result");
        assertTrue(run.getAsJsonArray("series").size() > 40);

        assertTrue(c.call("shutdown", null).get("result").isJsonNull());
        c.notifyEngine("exit", null);
        assertTrue(p.waitFor(30, TimeUnit.SECONDS), "engine exited");
        assertEquals(0, p.exitValue(), "exit code; stderr:\n" + Files.readString(stderr));

        synchronized (c.rawLines) {
            assertEquals(4, c.rawLines.size(), "stdout carried only the 4 responses");
            c.rawLines.forEach(l -> assertTrue(l.startsWith("{\"jsonrpc\":\"2.0\""), l));
        }
        System.err.println("engine process: initialize + load in " + readyMs + " ms; stderr " + new File(stderr.toString()).length() + " bytes");
    }
}
