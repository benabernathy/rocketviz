package net.rocketviz.engine.openrocket.rpc;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;

/** Handles one JSON-RPC method. */
@FunctionalInterface
public interface Handler {
    /**
     * @param params the request's params object (empty if the request had none)
     * @return the result; {@code JsonNull.INSTANCE} for a null result
     */
    JsonElement handle(JsonObject params, RequestContext ctx) throws RpcException;
}
