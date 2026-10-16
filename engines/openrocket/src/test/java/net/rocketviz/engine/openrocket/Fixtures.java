package net.rocketviz.engine.openrocket;

import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;

final class Fixtures {
    private Fixtures() {}

    /** Fizz.ork copied to a real file (OpenRocket loads from a path). */
    static Path fizz() throws IOException {
        Path p = Files.createTempFile("Fizz", ".ork");
        p.toFile().deleteOnExit();
        try (InputStream in = Fixtures.class.getResourceAsStream("/Fizz.ork")) {
            Files.copy(in, p, StandardCopyOption.REPLACE_EXISTING);
        }
        return p;
    }

    /** One of the example designs inside the OpenRocket JAR, copied to a real file. */
    static Path example(String name) throws IOException {
        Path p = Files.createTempFile("example", ".ork");
        p.toFile().deleteOnExit();
        try (InputStream in = Fixtures.class.getResourceAsStream("/datafiles/examples/" + name)) {
            if (in == null) throw new IOException("no example " + name);
            Files.copy(in, p, StandardCopyOption.REPLACE_EXISTING);
        }
        return p;
    }

    static JsonObject golden() throws IOException {
        try (InputStream in = Fixtures.class.getResourceAsStream("/golden/fizz.json")) {
            return JsonParser.parseReader(new InputStreamReader(in, StandardCharsets.UTF_8)).getAsJsonObject();
        }
    }
}
