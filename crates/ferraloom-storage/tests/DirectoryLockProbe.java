import java.nio.file.Path;
import net.minecraft.util.DirectoryLock;

/** Checks that Rust's record locks and Java's FileChannel locks conflict. */
public class DirectoryLockProbe {
    public static void main(String[] args) throws Exception {
        Path dir = Path.of(args[0]);
        boolean expected = Boolean.parseBoolean(args[1]);
        if (DirectoryLock.isLocked(dir) != expected) {
            throw new AssertionError("Java/Rust session.lock ownership disagrees");
        }
        if (!expected) {
            try (DirectoryLock lock = DirectoryLock.create(dir)) {
                if (!lock.isValid()) throw new AssertionError("Java could not acquire released lock");
            }
        }
    }
}
