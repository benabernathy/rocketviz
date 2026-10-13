package net.rocketviz.engine.openrocket.rpc;

import com.google.gson.Gson;
import com.google.gson.GsonBuilder;
import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import com.google.gson.JsonParseException;
import com.google.gson.JsonParser;
import com.google.gson.JsonPrimitive;
import java.io.BufferedReader;
import java.io.IOException;
import java.io.Reader;
import java.io.UncheckedIOException;
import java.io.Writer;
import java.util.Map;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.TimeUnit;

/**
 * JSON-RPC 2.0 over newline-delimited JSON (docs/engine-protocol.md §2).
 *
 * <p>The calling thread reads messages; requests run one at a time on a worker thread, so the reader stays free to
 * act on {@code $/cancelRequest} and {@code exit} while a long request (a Monte Carlo batch) is running.
 */
public final class JsonRpcServer {
    // serializeNulls: JSON-RPC requires "result": null, and missing values are null, not absent.
    private static final Gson GSON = new GsonBuilder().disableHtmlEscaping().serializeNulls().create();

    private final BufferedReader in;
    private final Writer out;
    private final Map<String, Handler> handlers;
    private final Map<JsonElement, RequestContext> active = new ConcurrentHashMap<>();
    private final ExecutorService worker = Executors.newSingleThreadExecutor(r -> {
        Thread t = new Thread(r, "rpc-worker");
        t.setDaemon(true);
        return t;
    });
    private volatile boolean shutdownRequested;

    public JsonRpcServer(Reader in, Writer out, Map<String, Handler> handlers) {
        this.in = new BufferedReader(in);
        this.out = out;
        this.handlers = Map.copyOf(handlers);
    }

    /**
     * Serves until {@code exit} or end of input.
     *
     * @return the process exit code: 0 after a clean shutdown or end of input, 1 for {@code exit} without
     *     {@code shutdown}
     */
    public int serve() throws IOException {
        String line;
        while ((line = in.readLine()) != null) {
            if (line.isBlank()) {
                continue;
            }
            JsonElement msg;
            try {
                msg = JsonParser.parseString(line);
            } catch (JsonParseException e) {
                sendError(JsonNull.INSTANCE, RpcException.PARSE_ERROR, "Parse error: " + e.getMessage(), null);
                continue;
            }
            if (!msg.isJsonObject()) {
                sendError(JsonNull.INSTANCE, RpcException.INVALID_REQUEST, "Expected a JSON object (batches are not supported)", null);
                continue;
            }
            JsonObject o = msg.getAsJsonObject();
            boolean isRequest = o.has("id");
            JsonElement id = isRequest ? o.get("id") : JsonNull.INSTANCE;
            String method = stringField(o, "method");
            JsonElement rawParams = o.get("params");

            if (!"2.0".equals(stringField(o, "jsonrpc")) || method == null || (isRequest && !validId(id))) {
                if (isRequest) {
                    sendError(validId(id) ? id : JsonNull.INSTANCE, RpcException.INVALID_REQUEST, "Invalid JSON-RPC 2.0 request", null);
                }
                continue;
            }
            if (rawParams != null && !rawParams.isJsonObject() && !rawParams.isJsonNull()) {
                if (isRequest) {
                    sendError(id, RpcException.INVALID_PARAMS, "params must be an object", null);
                }
                continue;
            }
            JsonObject params = rawParams != null && rawParams.isJsonObject() ? rawParams.getAsJsonObject() : new JsonObject();

            if (!isRequest) {
                if (method.equals("$/cancelRequest")) {
                    RequestContext ctx = params.has("id") ? active.get(params.get("id")) : null;
                    if (ctx != null) {
                        ctx.cancel();
                    }
                } else if (method.equals("exit")) {
                    return finish(shutdownRequested ? 0 : 1);
                }
                continue; // other notifications are ignored, per JSON-RPC
            }

            RequestContext ctx = new RequestContext(id, this);
            active.put(id, ctx);
            worker.execute(() -> dispatch(method, params, ctx));
        }
        return finish(0);
    }

    private void dispatch(String method, JsonObject params, RequestContext ctx) {
        try {
            Handler handler = handlers.get(method);
            if (handler == null) {
                throw new RpcException(RpcException.METHOD_NOT_FOUND, "Method not found: " + method);
            }
            if (shutdownRequested) {
                throw new RpcException(RpcException.INVALID_REQUEST, "Engine is shutting down");
            }
            JsonElement result = handler.handle(params, ctx);
            if (method.equals("shutdown")) {
                shutdownRequested = true;
            }
            sendResult(ctx.id(), result == null ? JsonNull.INSTANCE : result);
        } catch (RpcException e) {
            sendError(ctx.id(), e.code(), e.getMessage(), e.data());
        } catch (RuntimeException | Error e) {
            e.printStackTrace(System.err);
            sendError(ctx.id(), RpcException.INTERNAL_ERROR, "Internal error: " + e, null);
        } finally {
            active.remove(ctx.id());
        }
    }

    private int finish(int code) {
        active.values().forEach(RequestContext::cancel);
        worker.shutdown();
        try {
            if (!worker.awaitTermination(30, TimeUnit.SECONDS)) {
                System.err.println("rocketviz engine: a request did not finish within 30 s of exit");
            }
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
        }
        return code;
    }

    void sendNotification(String method, JsonObject params) {
        JsonObject msg = envelope();
        msg.addProperty("method", method);
        msg.add("params", params);
        send(msg);
    }

    private void sendResult(JsonElement id, JsonElement result) {
        JsonObject msg = envelope();
        msg.add("id", id);
        msg.add("result", result);
        send(msg);
    }

    private void sendError(JsonElement id, int code, String message, JsonElement data) {
        JsonObject err = new JsonObject();
        err.addProperty("code", code);
        err.addProperty("message", message == null ? "" : message);
        if (data != null) {
            err.add("data", data);
        }
        JsonObject msg = envelope();
        msg.add("id", id);
        msg.add("error", err);
        send(msg);
    }

    private synchronized void send(JsonObject msg) {
        try {
            out.write(GSON.toJson(msg));
            out.write('\n');
            out.flush();
        } catch (IOException e) {
            throw new UncheckedIOException("Lost connection to host", e);
        }
    }

    private static JsonObject envelope() {
        JsonObject msg = new JsonObject();
        msg.addProperty("jsonrpc", "2.0");
        return msg;
    }

    private static String stringField(JsonObject o, String key) {
        JsonElement e = o.get(key);
        return e != null && e.isJsonPrimitive() && e.getAsJsonPrimitive().isString() ? e.getAsString() : null;
    }

    private static boolean validId(JsonElement id) {
        if (id == null || id.isJsonNull()) {
            return false;
        }
        if (!id.isJsonPrimitive()) {
            return false;
        }
        JsonPrimitive p = id.getAsJsonPrimitive();
        return p.isString() || p.isNumber();
    }
}
