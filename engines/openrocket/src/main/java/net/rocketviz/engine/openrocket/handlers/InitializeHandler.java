package net.rocketviz.engine.openrocket.handlers;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import net.rocketviz.engine.openrocket.EngineInfo;
import net.rocketviz.engine.openrocket.rpc.Handler;
import net.rocketviz.engine.openrocket.rpc.RequestContext;
import net.rocketviz.engine.openrocket.rpc.RpcException;

/** {@code initialize}: protocol handshake; returns the engine's identity, capabilities and parameters. */
public final class InitializeHandler implements Handler {
    @Override
    public JsonElement handle(JsonObject params, RequestContext ctx) throws RpcException {
        JsonElement v = params.get("protocol_version");
        if (v == null || !v.isJsonPrimitive() || !v.getAsJsonPrimitive().isNumber() || v.getAsInt() != EngineInfo.PROTOCOL_VERSION) {
            throw new RpcException(RpcException.UNSUPPORTED_PROTOCOL,
                "This engine speaks protocol version " + EngineInfo.PROTOCOL_VERSION + ", host sent " + v);
        }
        return EngineInfo.describe();
    }
}
