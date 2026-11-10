package cloud.smarttasks.smartsavant;

import java.util.List;
import java.util.Map;

/**
 * The one boundary between the engine and a model (bindings/council.d.ts). An adapter answers two calls:
 * <pre>
 *   view(stakeholder, role, phaseId, question, context)  -> a ViewAnswer as JSON data (Map, List, String, ...)
 *   merge(phaseId, question, views, context)             -> a MergeAnswer as JSON data
 * </pre>
 * {@code context} is a fresh copy for every call: {idea, iteration, sources, prior, locked}. {@code views}
 * are the valid views of the phase as maps. The engine copies the answer to plain data, validates it,
 * retries a call that throws or answers invalidly, and records a failure instead of inventing content.
 * Return {@code true} from {@link #synthetic()} for adapters whose output is not a real deliberation.
 */
public interface LLMAdapter {
    /** Shown in council.json as "adapter". */
    default String name() { return "adapter"; }

    /** True for stubs and demos: their output is never learned from or built on. */
    default boolean synthetic() { return false; }

    Object view(String stakeholder, String role, String phaseId, String question, Map<String, Object> context) throws Exception;

    Object merge(String phaseId, String question, List<Map<String, Object>> views, Map<String, Object> context) throws Exception;
}
