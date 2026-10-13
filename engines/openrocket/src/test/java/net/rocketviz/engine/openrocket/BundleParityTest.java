package net.rocketviz.engine.openrocket;

import static net.rocketviz.engine.openrocket.RpcClient.obj;
import static org.junit.jupiter.api.Assertions.assertEquals;
import static org.junit.jupiter.api.Assertions.assertTrue;
import static org.junit.jupiter.api.Assumptions.assumeTrue;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.BufferedReader;
import java.io.File;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.ArrayList;
import java.util.Collections;
import java.util.List;
import java.util.Map;
import java.util.TreeMap;
import java.util.concurrent.TimeUnit;
import java.util.zip.ZipFile;
import org.junit.jupiter.api.Test;

/**
 * The engine as the desktop app ships it (trimmed jlink runtime, OpenRocket JAR without native libraries, the JVM
 * flags in engine.json) must simulate exactly like a full JDK with the stock JAR. Runs every example design that comes
 * with OpenRocket, plus Fizz. Needs `./gradlew bundle` first; skipped otherwise (CI runs it).
 */
class BundleParityTest {
    @Test
    void bundledEngineMatchesFullJdk() throws Exception {
        Path build = Path.of(System.getProperty("bundle.build", "build"));
        Path engineDir = build.resolve("engine"), jre = build.resolve("jre");
        String exe = System.getProperty("os.name").startsWith("Windows") ? "java.exe" : "java";
        assumeTrue(Files.isRegularFile(engineDir.resolve("engine.json")) && Files.isRegularFile(jre.resolve("bin").resolve(exe)),
            "run ./gradlew bundle first");

        List<Path> designs = exampleDesigns();
        designs.add(Fixtures.fizz());
        assertTrue(designs.size() > 10, "found OpenRocket's examples");

        // Reference: this JVM's full JDK, stock OpenRocket JAR (the test classpath).
        String fullJava = Path.of(System.getProperty("java.home"), "bin", exe).toString();
        Map<String, String> reference = simulate(List.of(fullJava, "-Djava.awt.headless=true", "-Xmx1g",
            "-cp", System.getProperty("java.class.path"), Main.class.getName()), designs);
        // Shipped: the bundled runtime and engine folder, launched the way engine.json says.
        List<String> shipped = new ArrayList<>(List.of(jre.resolve("bin").resolve(exe).toString()));
        shipped.addAll(launchArgs(engineDir));
        Map<String, String> bundled = simulate(shipped, designs);

        assertEquals(reference.keySet(), bundled.keySet());
        reference.forEach((k, v) -> assertEquals(v, bundled.get(k), k));
        long errors = reference.values().stream().filter(v -> v.startsWith("ERROR")).count();
        assertEquals(0, errors, "every example simulates: " + reference);
        System.err.println("bundle parity: " + reference.size() + " configs from " + designs.size() + " designs identical");
    }

    /** OpenRocket's own example designs, from its JAR. */
    private static List<Path> exampleDesigns() throws Exception {
        String jar = List.of(System.getProperty("java.class.path").split(File.pathSeparator)).stream()
            .filter(p -> p.contains("OpenRocket-")).findFirst().orElseThrow();
        Path dir = Files.createTempDirectory("or-examples");
        List<Path> out = new ArrayList<>();
        try (ZipFile z = new ZipFile(jar)) {
            for (var e : Collections.list(z.entries())) {
                if (!e.getName().startsWith("datafiles/examples/") || !e.getName().endsWith(".ork")) continue;
                Path p = dir.resolve(Path.of(e.getName()).getFileName().toString());
                try (var in = z.getInputStream(e)) { Files.copy(in, p); }
                p.toFile().deleteOnExit();
                out.add(p);
            }
        }
        Collections.sort(out);
        return out;
    }

    private static List<String> launchArgs(Path engineDir) throws Exception {
        JsonObject manifest = JsonParser.parseString(Files.readString(engineDir.resolve("engine.json"))).getAsJsonObject();
        List<String> args = new ArrayList<>();
        for (JsonElement a : manifest.getAsJsonObject("launch").getAsJsonArray("args")) {
            args.add(a.getAsString().replace("${engine_dir}", engineDir.toAbsolutePath().toString())
                .replace("${path_sep}", File.pathSeparator));
        }
        return args;
    }

    /** "design / config" -> summary JSON (or ERROR ...), for every config of every design. */
    private static Map<String, String> simulate(List<String> command, List<Path> designs) throws Exception {
        Path stderr = Files.createTempFile("engine-stderr", ".log");
        Process p = new ProcessBuilder(command).redirectError(stderr.toFile()).start();
        RpcClient c = new RpcClient(
            new BufferedReader(new InputStreamReader(p.getInputStream(), StandardCharsets.UTF_8)),
            new OutputStreamWriter(p.getOutputStream(), StandardCharsets.UTF_8));
        Map<String, String> out = new TreeMap<>();
        try {
            assertTrue(c.call("initialize", obj("protocol_version", 1)).has("result"));
            for (Path d : designs) {
                JsonObject load = c.call("load", obj("path", d.toString()));
                String name = d.getFileName().toString();
                if (!load.has("result")) { out.put(name, "ERROR " + load.get("error")); continue; }
                JsonObject design = load.getAsJsonObject("result");
                JsonArray configs = design.getAsJsonArray("configs");
                for (JsonElement cfg : configs) {
                    String id = cfg.getAsJsonObject().get("id").getAsString();
                    JsonObject run = c.call("run", obj("design_id", design.get("design_id").getAsString(), "config_id", id,
                        "overrides", obj("seed", 1), "output", "summary"));
                    String key = name + " / " + cfg.getAsJsonObject().get("name").getAsString() + " #" + id;
                    out.put(key, run.has("result") ? run.getAsJsonObject("result").toString() : "ERROR " + run.get("error"));
                }
            }
            c.call("shutdown", null);
            c.notifyEngine("exit", null);
            assertTrue(p.waitFor(30, TimeUnit.SECONDS), "engine exited");
        } finally {
            p.destroyForcibly();
        }
        String log = Files.readString(stderr);
        assertTrue(!log.contains("Exception") && !log.contains("Error"), command.get(0) + " logged problems:\n" + log);
        return out;
    }
}
