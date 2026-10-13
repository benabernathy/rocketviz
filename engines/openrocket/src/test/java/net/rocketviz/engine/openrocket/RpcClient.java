package net.rocketviz.engine.openrocket;

import static org.junit.jupiter.api.Assertions.fail;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import com.google.gson.JsonParser;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.Writer;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.LinkedBlockingQueue;
import java.util.concurrent.TimeUnit;

/** Test-side JSON-RPC client. Every line the engine writes must be a JSON-RPC message; notifications are kept. */
final class RpcClient {
    private final Writer out;
    private final BlockingQueue<JsonObject> incoming = new LinkedBlockingQueue<>();
    final List<JsonObject> notifications = new ArrayList<>();
    final List<String> rawLines = new ArrayList<>();
    private int nextId = 1;

    RpcClient(BufferedReader in, Writer out) {
        this.out = out;
        Thread reader = new Thread(() -> {
            try {
                String line;
                while ((line = in.readLine()) != null) {
                    synchronized (rawLines) { rawLines.add(line); }
                    JsonElement e = JsonParser.parseString(line); // throws if the engine wrote non-JSON
                    incoming.add(e.getAsJsonObject());
                }
            } catch (IOException e) {
                // stream closed
            }
        }, "rpc-client-reader");
        reader.setDaemon(true);
        reader.start();
    }

    int send(String method, JsonObject params) throws IOException {
        int id = nextId++;
        JsonObject msg = new JsonObject();
        msg.addProperty("jsonrpc", "2.0");
        msg.addProperty("id", id);
        msg.addProperty("method", method);
        if (params != null) msg.add("params", params);
        writeLine(msg.toString());
        return id;
    }

    void notifyEngine(String method, JsonObject params) throws IOException {
        JsonObject msg = new JsonObject();
        msg.addProperty("jsonrpc", "2.0");
        msg.addProperty("method", method);
        if (params != null) msg.add("params", params);
        writeLine(msg.toString());
    }

    void writeLine(String line) throws IOException {
        out.write(line);
        out.write('\n');
        out.flush();
    }

    /** Waits for the response to {@code id}, collecting notifications that arrive first. */
    JsonObject await(int id) throws InterruptedException {
        long deadline = System.nanoTime() + TimeUnit.SECONDS.toNanos(120);
        while (System.nanoTime() < deadline) {
            JsonObject m = incoming.poll(1, TimeUnit.SECONDS);
            if (m == null) continue;
            if (!m.has("id")) {
                notifications.add(m);
            } else if (m.get("id").isJsonPrimitive() && m.get("id").getAsInt() == id) {
                return m;
            } else {
                fail("Unexpected response: " + m);
            }
        }
        return fail("No response to request " + id);
    }

    /** Waits for the next message of any kind. */
    JsonObject next() throws InterruptedException {
        JsonObject m = incoming.poll(120, TimeUnit.SECONDS);
        return m != null ? m : fail("No message from engine");
    }

    JsonObject call(String method, JsonObject params) throws IOException, InterruptedException {
        return await(send(method, params));
    }

    static JsonObject obj(Object... kv) {
        JsonObject o = new JsonObject();
        for (int i = 0; i < kv.length; i += 2) {
            Object v = kv[i + 1];
            if (v instanceof JsonElement e) o.add((String) kv[i], e);
            else if (v instanceof Number n) o.addProperty((String) kv[i], n);
            else if (v instanceof Boolean b) o.addProperty((String) kv[i], b);
            else o.addProperty((String) kv[i], String.valueOf(v));
        }
        return o;
    }
}
