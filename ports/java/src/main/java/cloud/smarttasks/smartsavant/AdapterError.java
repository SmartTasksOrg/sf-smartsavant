package cloud.smarttasks.smartsavant;

/** What the scripted adapter throws for a {"raise": message} entry; the engine records "AdapterError: message". */
public class AdapterError extends RuntimeException {
    private static final long serialVersionUID = 1L;

    public AdapterError(String message) { super(message); }
}
