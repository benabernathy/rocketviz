package net.rocketviz.engine.openrocket.handlers;

import com.google.gson.JsonArray;
import com.google.gson.JsonElement;
import com.google.gson.JsonObject;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ExecutionException;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.atomic.AtomicInteger;
import net.rocketviz.engine.openrocket.EngineInfo;
import net.rocketviz.engine.openrocket.OpenRocketEngine;
import net.rocketviz.engine.openrocket.Output;
import net.rocketviz.engine.openrocket.rpc.Handler;
import net.rocketviz.engine.openrocket.rpc.RequestContext;
import net.rocketviz.engine.openrocket.rpc.RpcException;

/**
 * {@code run_batch}: runs many cases in parallel (Monte Carlo, wind sweeps), sending a {@code progress} notification
 * as each finishes. Results come back in case order; a failed case is reported, not fatal.
 */
public final class RunBatchHandler implements Handler {
    private final OpenRocketEngine engine;

    public RunBatchHandler(OpenRocketEngine engine) {
        this.engine = engine;
    }

    private record Case(String id, JsonObject overrides) {}

    /** Outcome of one case: a result, or the reason it failed (null if cancelled before it started). */
    private record Outcome(JsonObject result, String failure) {}

    @Override
    public JsonElement handle(JsonObject params, RequestContext ctx) throws RpcException {
        String designId = Params.string(params, "design_id");
        String configId = Params.string(params, "config_id");
        Output level = Output.parse(Params.optString(params, "output"));
        List<Case> cases = parseCases(params);

        int workers = Math.min(EngineInfo.MAX_WORKERS, Math.max(1, cases.size()));
        ExecutorService pool = Executors.newFixedThreadPool(workers, r -> {
            Thread t = new Thread(r, "batch-worker");
            t.setDaemon(true);
            return t;
        });
        AtomicInteger done = new AtomicInteger();
        List<Future<Outcome>> futures = new ArrayList<>(cases.size());
        try {
            for (Case c : cases) {
                futures.add(pool.submit(() -> {
                    if (ctx.isCancelled()) return new Outcome(null, null);
                    Outcome outcome;
                    try {
                        outcome = new Outcome(engine.run(designId, configId, c.overrides(), level), null);
                    } catch (RpcException e) {
                        outcome = new Outcome(null, e.getMessage());
                    }
                    JsonObject p = new JsonObject();
                    p.add("request_id", ctx.id());
                    p.addProperty("case_id", c.id());
                    p.addProperty("done", done.incrementAndGet());
                    p.addProperty("total", cases.size());
                    if (outcome.result() != null) p.add("result", outcome.result());
                    else p.addProperty("error", outcome.failure());
                    ctx.notify("progress", p);
                    return outcome;
                }));
            }

            JsonArray results = new JsonArray();
            JsonArray failed = new JsonArray();
            for (int i = 0; i < cases.size(); i++) {
                Outcome o = futures.get(i).get();
                if (o.result() != null) {
                    JsonObject r = new JsonObject();
                    r.addProperty("case_id", cases.get(i).id());
                    r.add("result", o.result());
                    results.add(r);
                } else if (o.failure() != null) {
                    JsonObject f = new JsonObject();
                    f.addProperty("case_id", cases.get(i).id());
                    f.addProperty("message", o.failure());
                    failed.add(f);
                }
            }
            ctx.checkCancelled();
            JsonObject out = new JsonObject();
            out.add("results", results);
            out.add("failed", failed);
            return out;
        } catch (InterruptedException e) {
            Thread.currentThread().interrupt();
            throw new RpcException(RpcException.REQUEST_CANCELLED, "Interrupted");
        } catch (ExecutionException e) {
            throw new RpcException(RpcException.INTERNAL_ERROR, "Batch worker failed: " + e.getCause());
        } finally {
            pool.shutdownNow();
        }
    }

    private static List<Case> parseCases(JsonObject params) throws RpcException {
        JsonElement raw = params.get("cases");
        if (raw == null || !raw.isJsonArray()) {
            throw new RpcException(RpcException.INVALID_PARAMS, "cases (array) is required");
        }
        List<Case> cases = new ArrayList<>();
        int i = 0;
        for (JsonElement e : raw.getAsJsonArray()) {
            if (!e.isJsonObject()) throw new RpcException(RpcException.INVALID_PARAMS, "cases[" + i + "] must be an object");
            JsonObject o = e.getAsJsonObject();
            String id = Params.optString(o, "id");
            cases.add(new Case(id != null ? id : String.valueOf(i), Params.optObject(o, "overrides")));
            i++;
        }
        return cases;
    }
}
