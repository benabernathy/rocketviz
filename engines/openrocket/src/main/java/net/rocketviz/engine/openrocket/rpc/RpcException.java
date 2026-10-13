package net.rocketviz.engine.openrocket.rpc;

import com.google.gson.JsonElement;
import com.google.gson.JsonObject;

/** A JSON-RPC error to return to the host. Codes are defined in docs/engine-protocol.md §6. */
public final class RpcException extends Exception {
    private static final long serialVersionUID = 1L;

    public static final int PARSE_ERROR = -32700;
    public static final int INVALID_REQUEST = -32600;
    public static final int METHOD_NOT_FOUND = -32601;
    public static final int INVALID_PARAMS = -32602;
    public static final int INTERNAL_ERROR = -32603;
    public static final int UNSUPPORTED_PROTOCOL = -32010;
    public static final int LOAD_FAILED = -32011;
    public static final int UNKNOWN_DESIGN = -32012;
    public static final int SIMULATION_FAILED = -32013;
    public static final int BAD_OVERRIDE = -32014;
    public static final int REQUEST_CANCELLED = -32800;

    private final int code;
    private final transient JsonElement data;

    public RpcException(int code, String message) {
        this(code, message, null);
    }

    public RpcException(int code, String message, JsonElement data) {
        super(message);
        this.code = code;
        this.data = data;
    }

    /** An error whose data is a single string field, e.g. {@code {"key": "wind_speed"}}. */
    public static RpcException withField(int code, String message, String field, String value) {
        JsonObject data = new JsonObject();
        data.addProperty(field, value);
        return new RpcException(code, message, data);
    }

    public int code() {
        return code;
    }

    public JsonElement data() {
        return data;
    }
}
