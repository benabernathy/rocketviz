package net.rocketviz.engine.openrocket.rpc;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import java.util.concurrent.atomic.AtomicBoolean;

/** Per-request state: its id, whether the host cancelled it, and a way to send notifications. */
public final class RequestContext {
    private final JsonElement id;
    private final AtomicBoolean cancelled = new AtomicBoolean();
    private final JsonRpcServer server;

    RequestContext(JsonElement id, JsonRpcServer server) {
        this.id = id;
        this.server = server;
    }

    public JsonElement id() {
        return id;
    }

    public boolean isCancelled() {
        return cancelled.get();
    }

    void cancel() {
        cancelled.set(true);
    }

    /** Throws the protocol's "request cancelled" error if the host cancelled this request. */
    public void checkCancelled() throws RpcException {
        if (isCancelled()) {
            throw new RpcException(RpcException.REQUEST_CANCELLED, "Request cancelled");
        }
    }

    /** Sends a notification to the host (safe to call from worker threads). */
    public void notify(String method, JsonObject params) {
        server.sendNotification(method, params);
    }
}
