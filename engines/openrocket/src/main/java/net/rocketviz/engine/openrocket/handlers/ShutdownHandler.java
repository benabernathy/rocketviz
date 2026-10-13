package net.rocketviz.engine.openrocket.handlers;

import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import net.rocketviz.engine.openrocket.rpc.Handler;
import net.rocketviz.engine.openrocket.rpc.RequestContext;

/** {@code shutdown}: acknowledges; the server then refuses new work until {@code exit}. */
public final class ShutdownHandler implements Handler {
    @Override
    public JsonElement handle(JsonObject params, RequestContext ctx) {
        return JsonNull.INSTANCE;
    }
}
