package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubBukkitScheduler;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.PriorityQueue;
import java.util.concurrent.Callable;
import java.util.concurrent.ConcurrentHashMap;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.concurrent.Future;
import java.util.concurrent.FutureTask;
import java.util.concurrent.atomic.AtomicInteger;
import java.util.function.Consumer;
import java.util.logging.Level;
import org.bukkit.plugin.IllegalPluginAccessException;
import org.bukkit.plugin.Plugin;
import org.bukkit.scheduler.BukkitRunnable;
import org.bukkit.scheduler.BukkitTask;
import org.bukkit.scheduler.BukkitWorker;

/**
 * BukkitScheduler with CraftScheduler's timing: a task scheduled with
 * delay {@code d} while the last heartbeat was tick {@code t} is due at
 * {@code t + d} and runs in the first heartbeat at or after it, never in the
 * heartbeat it was scheduled in. Sync tasks run on the server thread in the
 * heartbeat; async tasks are handed to a thread pool when due.
 */
final class FerraloomScheduler extends StubBukkitScheduler {
    /** CraftTask.NO_REPEATING. */
    private static final long ONCE = -1;

    private final AtomicInteger ids = new AtomicInteger(1);
    private final PriorityQueue<Task> queue = new PriorityQueue<>(
            Comparator.comparingLong((Task t) -> t.nextRun).thenComparingInt(t -> t.id));
    private final Map<Integer, Task> tasks = new ConcurrentHashMap<>();
    private final Map<Integer, Worker> workers = new ConcurrentHashMap<>();
    private final AtomicInteger threads = new AtomicInteger();
    private final ExecutorService pool = Executors.newCachedThreadPool(r -> {
        Thread t = new Thread(r, "Ferraloom Scheduler Thread - " + threads.incrementAndGet());
        t.setDaemon(true);
        return t;
    });
    private volatile long currentTick = -1;
    private volatile Task running;

    /** CraftScheduler.mainThreadHeartbeat: what is due at {@code tick}, in due order. */
    void heartbeat(long tick) {
        currentTick = tick;
        List<Task> due = new ArrayList<>();
        synchronized (queue) {
            while (!queue.isEmpty() && queue.peek().nextRun <= tick) {
                due.add(queue.poll());
            }
        }
        for (Task task : due) {
            if (task.cancelled) {
                tasks.remove(task.id, task);
                continue;
            }
            if (task.async) {
                Worker worker = new Worker(task);
                workers.put(task.id, worker);
                pool.execute(worker);
            } else {
                running = task;
                try {
                    task.body.run();
                } catch (Throwable ex) {
                    report(task, ex);
                } finally {
                    running = null;
                }
            }
            if (task.period > 0 && !task.cancelled) {
                task.nextRun = tick + task.period;
                synchronized (queue) {
                    queue.add(task);
                }
            } else if (!task.async) {
                tasks.remove(task.id, task);
            }
        }
    }

    void shutdown() {
        for (Task task : tasks.values()) {
            task.cancelled = true;
        }
        tasks.clear();
        synchronized (queue) {
            queue.clear();
        }
        pool.shutdown();
    }

    private void report(Task task, Throwable ex) {
        task.owner.getLogger().log(Level.WARNING, String.format("Task #%s for %s generated an exception",
                task.id, task.owner.getDescription().getFullName()), ex);
    }

    /** CraftScheduler.handle: the checks and the clamps, then the queue. */
    private Task schedule(Plugin plugin, Runnable body, Consumer<? super BukkitTask> consumer, long delay,
            long period, boolean async) {
        if (plugin == null) {
            throw new IllegalArgumentException("Plugin cannot be null");
        }
        if (body == null && consumer == null) {
            throw new IllegalArgumentException("Task cannot be null");
        }
        if (!plugin.isEnabled()) {
            throw new IllegalPluginAccessException("Plugin attempted to register task while disabled");
        }
        if (delay < 0) {
            delay = 0;
        }
        if (period == 0) {
            period = 1;
        } else if (period < ONCE) {
            period = ONCE;
        }
        Task task = new Task(ids.getAndIncrement(), plugin, async, period);
        task.body = body != null ? body : () -> consumer.accept(task);
        task.nextRun = currentTick + delay;
        tasks.put(task.id, task);
        synchronized (queue) {
            queue.add(task);
        }
        return task;
    }

    private static UnsupportedOperationException useRunnable() {
        return new UnsupportedOperationException("Use BukkitRunnable#runTask(Plugin)");
    }

    // BukkitTask runs.

    @Override
    public BukkitTask runTask(Plugin plugin, Runnable task) {
        return schedule(plugin, task, null, 0, ONCE, false);
    }

    @Override
    public void runTask(Plugin plugin, Consumer<? super BukkitTask> task) {
        schedule(plugin, null, task, 0, ONCE, false);
    }

    @Override
    public BukkitTask runTask(Plugin plugin, BukkitRunnable task) {
        throw useRunnable();
    }

    @Override
    public BukkitTask runTaskAsynchronously(Plugin plugin, Runnable task) {
        return schedule(plugin, task, null, 0, ONCE, true);
    }

    @Override
    public void runTaskAsynchronously(Plugin plugin, Consumer<? super BukkitTask> task) {
        schedule(plugin, null, task, 0, ONCE, true);
    }

    @Override
    public BukkitTask runTaskAsynchronously(Plugin plugin, BukkitRunnable task) {
        throw useRunnable();
    }

    @Override
    public BukkitTask runTaskLater(Plugin plugin, Runnable task, long delay) {
        return schedule(plugin, task, null, delay, ONCE, false);
    }

    @Override
    public void runTaskLater(Plugin plugin, Consumer<? super BukkitTask> task, long delay) {
        schedule(plugin, null, task, delay, ONCE, false);
    }

    @Override
    public BukkitTask runTaskLater(Plugin plugin, BukkitRunnable task, long delay) {
        throw useRunnable();
    }

    @Override
    public BukkitTask runTaskLaterAsynchronously(Plugin plugin, Runnable task, long delay) {
        return schedule(plugin, task, null, delay, ONCE, true);
    }

    @Override
    public void runTaskLaterAsynchronously(Plugin plugin, Consumer<? super BukkitTask> task, long delay) {
        schedule(plugin, null, task, delay, ONCE, true);
    }

    @Override
    public BukkitTask runTaskLaterAsynchronously(Plugin plugin, BukkitRunnable task, long delay) {
        throw useRunnable();
    }

    @Override
    public BukkitTask runTaskTimer(Plugin plugin, Runnable task, long delay, long period) {
        return schedule(plugin, task, null, delay, period, false);
    }

    @Override
    public void runTaskTimer(Plugin plugin, Consumer<? super BukkitTask> task, long delay, long period) {
        schedule(plugin, null, task, delay, period, false);
    }

    @Override
    public BukkitTask runTaskTimer(Plugin plugin, BukkitRunnable task, long delay, long period) {
        throw useRunnable();
    }

    @Override
    public BukkitTask runTaskTimerAsynchronously(Plugin plugin, Runnable task, long delay, long period) {
        return schedule(plugin, task, null, delay, period, true);
    }

    @Override
    public void runTaskTimerAsynchronously(Plugin plugin, Consumer<? super BukkitTask> task, long delay, long period) {
        schedule(plugin, null, task, delay, period, true);
    }

    @Override
    public BukkitTask runTaskTimerAsynchronously(Plugin plugin, BukkitRunnable task, long delay, long period) {
        throw useRunnable();
    }

    // The deprecated id-returning forms.

    @Override
    public int scheduleSyncDelayedTask(Plugin plugin, Runnable task, long delay) {
        return runTaskLater(plugin, task, delay).getTaskId();
    }

    @Override
    public int scheduleSyncDelayedTask(Plugin plugin, BukkitRunnable task, long delay) {
        throw useRunnable();
    }

    @Override
    public int scheduleSyncDelayedTask(Plugin plugin, Runnable task) {
        return runTask(plugin, task).getTaskId();
    }

    @Override
    public int scheduleSyncDelayedTask(Plugin plugin, BukkitRunnable task) {
        throw useRunnable();
    }

    @Override
    public int scheduleSyncRepeatingTask(Plugin plugin, Runnable task, long delay, long period) {
        return runTaskTimer(plugin, task, delay, period).getTaskId();
    }

    @Override
    public int scheduleSyncRepeatingTask(Plugin plugin, BukkitRunnable task, long delay, long period) {
        throw useRunnable();
    }

    @Override
    public int scheduleAsyncDelayedTask(Plugin plugin, Runnable task, long delay) {
        return runTaskLaterAsynchronously(plugin, task, delay).getTaskId();
    }

    @Override
    public int scheduleAsyncDelayedTask(Plugin plugin, Runnable task) {
        return runTaskAsynchronously(plugin, task).getTaskId();
    }

    @Override
    public int scheduleAsyncRepeatingTask(Plugin plugin, Runnable task, long delay, long period) {
        return runTaskTimerAsynchronously(plugin, task, delay, period).getTaskId();
    }

    /** CraftScheduler.callSyncMethod: the callable in the next heartbeat. */
    @Override
    public <T> Future<T> callSyncMethod(Plugin plugin, Callable<T> task) {
        FutureTask<T> future = new FutureTask<>(task);
        runTask(plugin, future);
        return future;
    }

    // Queries and cancelling.

    @Override
    public void cancelTask(int taskId) {
        Task task = tasks.remove(taskId);
        if (task != null) {
            task.cancelled = true;
        }
    }

    @Override
    public void cancelTasks(Plugin plugin) {
        for (Task task : tasks.values()) {
            if (task.owner.equals(plugin)) {
                cancelTask(task.id);
            }
        }
    }

    @Override
    public boolean isCurrentlyRunning(int taskId) {
        Task now = running;
        return (now != null && now.id == taskId) || workers.containsKey(taskId);
    }

    @Override
    public boolean isQueued(int taskId) {
        Task task = tasks.get(taskId);
        return task != null && !task.cancelled;
    }

    @Override
    public List<BukkitWorker> getActiveWorkers() {
        return List.copyOf(workers.values());
    }

    @Override
    public List<BukkitTask> getPendingTasks() {
        List<BukkitTask> pending = new ArrayList<>();
        for (Task task : tasks.values()) {
            if (!task.cancelled) {
                pending.add(task);
            }
        }
        return pending;
    }

    /** CraftTask: one scheduled body, its owner and its next due tick. */
    private final class Task implements BukkitTask {
        final int id;
        final Plugin owner;
        final boolean async;
        final long period;
        Runnable body;
        volatile long nextRun;
        volatile boolean cancelled;

        Task(int id, Plugin owner, boolean async, long period) {
            this.id = id;
            this.owner = owner;
            this.async = async;
            this.period = period;
        }

        @Override
        public int getTaskId() {
            return id;
        }

        @Override
        public Plugin getOwner() {
            return owner;
        }

        @Override
        public boolean isSync() {
            return !async;
        }

        @Override
        public boolean isCancelled() {
            return cancelled;
        }

        @Override
        public void cancel() {
            cancelTask(id);
        }
    }

    /** CraftAsyncTask's worker: an async task's run on the pool. */
    private final class Worker implements BukkitWorker, Runnable {
        final Task task;
        volatile Thread thread;

        Worker(Task task) {
            this.task = task;
        }

        @Override
        public void run() {
            thread = Thread.currentThread();
            try {
                task.body.run();
            } catch (Throwable ex) {
                report(task, ex);
            } finally {
                workers.remove(task.id, this);
                if (task.period <= 0) {
                    tasks.remove(task.id, task);
                }
            }
        }

        @Override
        public int getTaskId() {
            return task.id;
        }

        @Override
        public Plugin getOwner() {
            return task.owner;
        }

        @Override
        public Thread getThread() {
            return thread;
        }
    }
}
