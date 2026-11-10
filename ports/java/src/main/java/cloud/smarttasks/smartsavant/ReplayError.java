package cloud.smarttasks.smartsavant;

/** What the replay adapter throws for a view or merge the recorded run has no valid answer for. */
public class ReplayError extends RuntimeException {
    private static final long serialVersionUID = 1L;

    public ReplayError(String message) { super(message); }
}
