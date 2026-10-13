package net.rocketviz.engine.openrocket;

import com.google.gson.JsonObject;
import info.openrocket.core.util.BuildProperties;

/** The engine's identity and capabilities, as returned by {@code initialize}. */
public final class EngineInfo {
    private EngineInfo() {}

    public static final int PROTOCOL_VERSION = 1;
    public static final String ENGINE_VERSION = "0.1.0";
    /**
     * Batch worker threads per engine process. OpenRocket stops scaling past ~3-4 threads in one JVM (global state
     * inside its simulation code; measured 2.3x at 4 threads, 1.1x at 8), while separate processes scale nearly
     * linearly. So each process uses a few threads and the host runs several processes ({@code multi_instance}).
     * {@code -Drocketviz.workers=N} overrides it.
     */
    public static final int MAX_WORKERS = Integer.getInteger("rocketviz.workers",
        Math.max(1, Math.min(Runtime.getRuntime().availableProcessors(), 3)));

    public static JsonObject describe() {
        JsonObject engine = new JsonObject();
        engine.addProperty("id", "openrocket");
        engine.addProperty("name", "OpenRocket");
        engine.addProperty("version", BuildProperties.getVersion());
        engine.addProperty("bridge_version", ENGINE_VERSION);
        engine.addProperty("license", "GPL-3.0-or-later");

        JsonObject caps = new JsonObject();
        caps.addProperty("batch", true);
        caps.addProperty("parallel", true);
        caps.addProperty("max_workers", MAX_WORKERS);
        caps.addProperty("cancel", true);
        caps.addProperty("multi_instance", true);

        JsonObject out = new JsonObject();
        out.addProperty("protocol_version", PROTOCOL_VERSION);
        out.add("engine", engine);
        out.add("capabilities", caps);
        out.add("parameters", Parameters.describe());
        return out;
    }
}
