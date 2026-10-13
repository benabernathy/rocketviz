package net.rocketviz.engine.openrocket.handlers;

import com.google.gson.JsonElement;
import com.google.gson.JsonNull;
import com.google.gson.JsonObject;
import net.rocketviz.engine.openrocket.OpenRocketEngine;
import net.rocketviz.engine.openrocket.rpc.Handler;
import net.rocketviz.engine.openrocket.rpc.RequestContext;
import net.rocketviz.engine.openrocket.rpc.RpcException;

/** {@code unload}: forgets a loaded design. */
public final class UnloadHandler implements Handler {
    private final OpenRocketEngine engine;

    public UnloadHandler(OpenRocketEngine engine) {
        this.engine = engine;
    }

    @Override
    public JsonElement handle(JsonObject params, RequestContext ctx) throws RpcException {
        engine.unload(Params.string(params, "design_id"));
        return JsonNull.INSTANCE;
    }
}
