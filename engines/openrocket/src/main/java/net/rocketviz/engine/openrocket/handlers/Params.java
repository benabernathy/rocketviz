package net.rocketviz.engine.openrocket.handlers;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import net.rocketviz.engine.openrocket.rpc.RpcException;

/** Typed access to request params, with protocol errors for missing or mistyped fields. */
final class Params {
    private Params() {}

    static String string(JsonObject params, String key) throws RpcException {
        JsonElement e = params.get(key);
        if (e == null || !e.isJsonPrimitive() || !e.getAsJsonPrimitive().isString()) {
            throw new RpcException(RpcException.INVALID_PARAMS, key + " (string) is required");
        }
        return e.getAsString();
    }

    static String optString(JsonObject params, String key) throws RpcException {
        return params.has(key) && !params.get(key).isJsonNull() ? string(params, key) : null;
    }

    static JsonObject optObject(JsonObject params, String key) throws RpcException {
        JsonElement e = params.get(key);
        if (e == null || e.isJsonNull()) return new JsonObject();
        if (!e.isJsonObject()) throw new RpcException(RpcException.INVALID_PARAMS, key + " must be an object");
        return e.getAsJsonObject();
    }
}
