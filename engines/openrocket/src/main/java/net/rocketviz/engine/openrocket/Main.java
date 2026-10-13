package net.rocketviz.engine.openrocket;

import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.PrintStream;
import java.io.Writer;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import net.rocketviz.engine.openrocket.handlers.InitializeHandler;
import net.rocketviz.engine.openrocket.handlers.LoadHandler;
import net.rocketviz.engine.openrocket.handlers.RunBatchHandler;
import net.rocketviz.engine.openrocket.handlers.RunHandler;
import net.rocketviz.engine.openrocket.handlers.ShutdownHandler;
import net.rocketviz.engine.openrocket.handlers.UnloadHandler;
import net.rocketviz.engine.openrocket.rpc.Handler;
import net.rocketviz.engine.openrocket.rpc.JsonRpcServer;

/** Entry point: the OpenRocket engine process (docs/engine-protocol.md). */
public final class Main {
    private Main() {}

    public static void main(String[] args) throws Exception {
        // stdout belongs to the protocol. Point System.out at stderr *before* OpenRocket or its logging start, so
        // nothing they print can corrupt the message stream.
        Writer protocol = new OutputStreamWriter(new FileOutputStream(FileDescriptor.out), StandardCharsets.UTF_8);
        System.setOut(new PrintStream(new FileOutputStream(FileDescriptor.err), true, StandardCharsets.UTF_8));
        System.setProperty("java.awt.headless", "true");

        OpenRocketEngine.startAsync();
        JsonRpcServer server = new JsonRpcServer(
            new InputStreamReader(System.in, StandardCharsets.UTF_8), protocol, handlers(new OpenRocketEngine()));
        System.exit(server.serve());
    }

    public static Map<String, Handler> handlers(OpenRocketEngine engine) {
        return Map.of(
            "initialize", new InitializeHandler(),
            "load", new LoadHandler(engine),
            "unload", new UnloadHandler(engine),
            "run", new RunHandler(engine),
            "run_batch", new RunBatchHandler(engine),
            "shutdown", new ShutdownHandler());
    }
}
