package net.rocketviz.engine.openrocket.handlers;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import net.rocketviz.engine.openrocket.OpenRocketEngine;
import net.rocketviz.engine.openrocket.rpc.Handler;
import net.rocketviz.engine.openrocket.rpc.RequestContext;
import net.rocketviz.engine.openrocket.rpc.RpcException;

/** {@code load}: opens a design file and lists its configurations. */
public final class LoadHandler implements Handler {
    private final OpenRocketEngine engine;

    public LoadHandler(OpenRocketEngine engine) {
        this.engine = engine;
    }

    @Override
    public JsonElement handle(JsonObject params, RequestContext ctx) throws RpcException {
        return engine.load(Params.string(params, "path"));
    }
}
