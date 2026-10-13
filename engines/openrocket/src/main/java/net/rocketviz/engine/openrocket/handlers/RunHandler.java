package net.rocketviz.engine.openrocket.handlers;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import net.rocketviz.engine.openrocket.OpenRocketEngine;
import net.rocketviz.engine.openrocket.Output;
import net.rocketviz.engine.openrocket.rpc.Handler;
import net.rocketviz.engine.openrocket.rpc.RequestContext;
import net.rocketviz.engine.openrocket.rpc.RpcException;

/** {@code run}: simulates one case. */
public final class RunHandler implements Handler {
    private final OpenRocketEngine engine;

    public RunHandler(OpenRocketEngine engine) {
        this.engine = engine;
    }

    @Override
    public JsonElement handle(JsonObject params, RequestContext ctx) throws RpcException {
        return engine.run(
            Params.string(params, "design_id"),
            Params.string(params, "config_id"),
            Params.optObject(params, "overrides"),
            Output.parse(Params.optString(params, "output")));
    }
}
