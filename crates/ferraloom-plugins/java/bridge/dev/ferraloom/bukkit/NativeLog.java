package dev.ferraloom.bukkit;

import java.io.ByteArrayOutputStream;
import java.io.OutputStream;
import java.io.PrintStream;
import java.io.PrintWriter;
import java.io.StringWriter;
import java.nio.charset.StandardCharsets;
import java.util.logging.Handler;
import java.util.logging.Level;
import java.util.logging.LogManager;
import java.util.logging.LogRecord;
import java.util.logging.Logger;
import java.util.logging.SimpleFormatter;

/**
 * java.util.logging and System.out/err into the server's log, so plugins'
 * lines read as the server's own: {@code [12:00:00] [Server thread/INFO]: [MyPlugin] Enabling MyPlugin v1.0}.
 */
final class NativeLog extends Handler {
    private final SimpleFormatter formatter = new SimpleFormatter();

    /** Routes the JDK's root logger and the standard streams here; returns the server's logger. */
    static Logger install() {
        LogManager.getLogManager().reset();
        Logger root = Logger.getLogger("");
        root.setLevel(Level.INFO);
        root.addHandler(new NativeLog());
        System.setOut(new PrintStream(new Lines("INFO"), true, StandardCharsets.UTF_8));
        System.setErr(new PrintStream(new Lines("WARN"), true, StandardCharsets.UTF_8));
        return Logger.getLogger("Minecraft");
    }

    static String level(Level level) {
        if (level.intValue() >= Level.SEVERE.intValue()) {
            return "ERROR";
        }
        return level.intValue() >= Level.WARNING.intValue() ? "WARN" : "INFO";
    }

    @Override
    public void publish(LogRecord record) {
        if (!isLoggable(record)) {
            return;
        }
        String message = formatter.formatMessage(record);
        if (record.getThrown() != null) {
            StringWriter trace = new StringWriter();
            record.getThrown().printStackTrace(new PrintWriter(trace));
            message = (message == null || message.isEmpty() ? "" : message + "\n") + trace.toString().stripTrailing();
        }
        ServerNative.log(level(record.getLevel()), Thread.currentThread().getName(), message == null ? "" : message);
    }

    @Override
    public void flush() {
    }

    @Override
    public void close() {
    }

    /** A stream that logs each line written to it. */
    private static final class Lines extends OutputStream {
        private final String level;
        private final ByteArrayOutputStream line = new ByteArrayOutputStream();

        Lines(String level) {
            this.level = level;
        }

        @Override
        public synchronized void write(int b) {
            if (b == '\n') {
                emit();
            } else if (b != '\r') {
                line.write(b);
            }
        }

        @Override
        public synchronized void flush() {
            if (line.size() > 0) {
                emit();
            }
        }

        private void emit() {
            ServerNative.log(level, Thread.currentThread().getName(), line.toString(StandardCharsets.UTF_8));
            line.reset();
        }
    }
}
