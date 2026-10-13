package net.rocketviz.engine.openrocket;

import ch.qos.logback.classic.Level;
import ch.qos.logback.classic.Logger;
import com.google.gson.JsonArray;
import com.google.gson.JsonObject;
import com.google.inject.Guice;
import info.openrocket.core.document.OpenRocketDocument;
import info.openrocket.core.document.Simulation;
import info.openrocket.core.file.GeneralRocketLoader;
import info.openrocket.core.file.RocketLoadException;
import info.openrocket.core.plugin.PluginModule;
import info.openrocket.core.rocketcomponent.FlightConfiguration;
import info.openrocket.core.simulation.exception.SimulationException;
import info.openrocket.core.startup.Application;
import info.openrocket.swing.startup.GuiModule;
import java.io.File;
import java.util.List;
import java.util.Map;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.atomic.AtomicInteger;
import net.rocketviz.engine.openrocket.rpc.RpcException;
import org.slf4j.LoggerFactory;

/** OpenRocket behind the engine protocol: starts it headless, keeps loaded designs, runs simulations. */
public final class OpenRocketEngine {
    private static final CompletableFuture<Void> READY = new CompletableFuture<>();
    private static final AtomicInteger STARTED = new AtomicInteger();

    private final Map<String, OpenRocketDocument> designs = new ConcurrentHashMap<>();
    private final AtomicInteger nextDesign = new AtomicInteger(1);

    /**
     * Starts OpenRocket in the background (about 2.5 s) so it's warming up while the host connects. OpenRocket's
     * startup is global to the JVM, so only the first call does work.
     */
    public static void startAsync() {
        if (STARTED.getAndIncrement() > 0) return;
        Thread t = new Thread(() -> {
            try {
                ((Logger) LoggerFactory.getLogger(org.slf4j.Logger.ROOT_LOGGER_NAME)).setLevel(Level.ERROR);
                // GuiModule provides the component and motor database loaders OpenRocket needs even when headless.
                GuiModule gui = new GuiModule();
                Application.setInjector(Guice.createInjector(gui, new PluginModule()));
                gui.startLoader();
                READY.complete(null);
            } catch (Throwable e) {
                READY.completeExceptionally(e);
            }
        }, "openrocket-startup");
        t.setDaemon(true);
        t.start();
    }

    static void awaitReady() throws RpcException {
        startAsync();
        try {
            READY.get();
        } catch (ExecutionException e) {
            throw new RpcException(RpcException.INTERNAL_ERROR, "OpenRocket failed to start: " + e.getCause());
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RpcException(RpcException.INTERNAL_ERROR, "Interrupted while starting OpenRocket");
        }
    }

    /** Loads a design and describes its runnable configurations (OpenRocket simulations). */
    public JsonObject load(String path) throws RpcException {
        awaitReady();
        File file = new File(path);
        if (!file.isFile()) {
            throw RpcException.withField(RpcException.LOAD_FAILED, "No such file: " + path, "path", path);
        }
        OpenRocketDocument doc;
        try {
            doc = new GeneralRocketLoader(file).load();
        } catch (RocketLoadException | RuntimeException e) {
            throw RpcException.withField(RpcException.LOAD_FAILED, "Couldn't load " + file.getName() + ": " + e.getMessage(), "path", path);
        }
        String id = "d" + nextDesign.getAndIncrement();
        designs.put(id, doc);

        JsonObject out = new JsonObject();
        out.addProperty("design_id", id);
        out.addProperty("name", doc.getRocket().getName());
        JsonArray configs = new JsonArray();
        List<Simulation> sims = doc.getSimulations();
        for (int i = 0; i < sims.size(); i++) {
            Simulation sim = sims.get(i);
            FlightConfiguration fc = doc.getRocket().getFlightConfiguration(sim.getFlightConfigurationId());
            JsonObject c = new JsonObject();
            c.addProperty("id", String.valueOf(i));
            c.addProperty("name", sim.getName());
            c.addProperty("motor", fc != null ? fc.getName() : "");
            c.add("defaults", Parameters.read(sim.getOptions()));
            configs.add(c);
        }
        out.add("configs", configs);
        return out;
    }

    public void unload(String designId) throws RpcException {
        if (designs.remove(designId) == null) {
            throw RpcException.withField(RpcException.UNKNOWN_DESIGN, "Unknown design: " + designId, "design_id", designId);
        }
    }

    /**
     * Simulates one case. Safe to call concurrently: each call works on its own copy of the simulation.
     */
    public JsonObject run(String designId, String configId, JsonObject overrides, Output level) throws RpcException {
        Simulation sim = copyOf(designId, configId);
        Parameters.apply(sim.getOptions(), overrides);
        try {
            sim.simulate();
        } catch (SimulationException | RuntimeException e) {
            throw RpcException.withField(RpcException.SIMULATION_FAILED, "Simulation failed: " + e.getMessage(), "message", String.valueOf(e.getMessage()));
        }
        return Output.write(sim, level);
    }

    private Simulation copyOf(String designId, String configId) throws RpcException {
        OpenRocketDocument doc = designs.get(designId);
        if (doc == null) {
            throw RpcException.withField(RpcException.UNKNOWN_DESIGN, "Unknown design: " + designId, "design_id", designId);
        }
        List<Simulation> sims = doc.getSimulations();
        int index;
        try {
            index = Integer.parseInt(configId);
        } catch (NumberFormatException e) {
            index = -1;
        }
        if (index < 0 || index >= sims.size()) {
            throw RpcException.withField(RpcException.UNKNOWN_DESIGN, "Unknown config: " + configId, "config_id", configId);
        }
        // Simulation.copy() reads shared document state; serialize copies so parallel batch workers don't race.
        synchronized (doc) {
            return sims.get(index).copy();
        }
    }
}
