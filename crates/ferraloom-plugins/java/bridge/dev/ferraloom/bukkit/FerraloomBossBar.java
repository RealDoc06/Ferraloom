package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubBossBar;
import java.util.ArrayList;
import java.util.Collections;
import java.util.EnumSet;
import java.util.LinkedHashSet;
import java.util.List;
import java.util.Set;
import java.util.UUID;
import java.util.WeakHashMap;
import org.bukkit.boss.BarColor;
import org.bukkit.boss.BarFlag;
import org.bukkit.boss.BarStyle;
import org.bukkit.entity.Player;

/**
 * CraftBossBar over a ServerBossEvent: the state is here, and each change
 * hands all of it to the server (ServerNative.bossBar), which shows the bar
 * to its players from the next tick's boss bar update. Safe on any thread.
 * A player who leaves is taken off every bar, as their ServerPlayer is in
 * CraftBukkit (a reconnect starts with none).
 */
final class FerraloomBossBar extends StubBossBar {
    /** Every bar with players, to take a leaving player off (weak: a bar a plugin drops goes with its players' view of it). */
    private static final Set<FerraloomBossBar> WITH_PLAYERS = Collections.newSetFromMap(new WeakHashMap<>());

    private final UUID id = UUID.randomUUID();
    private String title;
    private BarColor color;
    private BarStyle style;
    private final EnumSet<BarFlag> flags = EnumSet.noneOf(BarFlag.class);
    private double progress = 1.0;
    private boolean visible = true;
    private final Set<Player> players = new LinkedHashSet<>();

    FerraloomBossBar(String title, BarColor color, BarStyle style, BarFlag... flags) {
        this.title = title == null ? "" : title;
        this.color = color;
        this.style = style;
        for (BarFlag f : flags) {
            this.flags.add(f);
        }
    }

    /** The server's copy, after every change. */
    private void publish() {
        long[] halves = new long[players.size() * 2];
        int i = 0;
        for (Player p : players) {
            halves[i++] = p.getUniqueId().getMostSignificantBits();
            halves[i++] = p.getUniqueId().getLeastSignificantBits();
        }
        int bits = (flags.contains(BarFlag.DARKEN_SKY) ? 1 : 0) | (flags.contains(BarFlag.PLAY_BOSS_MUSIC) ? 2 : 0)
                | (flags.contains(BarFlag.CREATE_FOG) ? 4 : 0) | (visible ? 8 : 0);
        ServerNative.bossBar(id.getMostSignificantBits(), id.getLeastSignificantBits(), title, (float) progress, color.ordinal(),
                style.ordinal(), bits, halves);
        synchronized (WITH_PLAYERS) {
            if (players.isEmpty()) {
                WITH_PLAYERS.remove(this);
            } else {
                WITH_PLAYERS.add(this);
            }
        }
    }

    /** A player left: off every bar. */
    static void left(Player player) {
        List<FerraloomBossBar> bars;
        synchronized (WITH_PLAYERS) {
            bars = new ArrayList<>(WITH_PLAYERS);
        }
        for (FerraloomBossBar bar : bars) {
            bar.removePlayer(player);
        }
    }

    @Override
    public synchronized String getTitle() {
        return title;
    }

    @Override
    public synchronized void setTitle(String title) {
        this.title = title == null ? "" : title;
        publish();
    }

    @Override
    public synchronized BarColor getColor() {
        return color;
    }

    @Override
    public synchronized void setColor(BarColor color) {
        this.color = color;
        publish();
    }

    @Override
    public synchronized BarStyle getStyle() {
        return style;
    }

    @Override
    public synchronized void setStyle(BarStyle style) {
        this.style = style;
        publish();
    }

    @Override
    public synchronized void removeFlag(BarFlag flag) {
        if (flags.remove(flag)) {
            publish();
        }
    }

    @Override
    public synchronized void addFlag(BarFlag flag) {
        if (flags.add(flag)) {
            publish();
        }
    }

    @Override
    public synchronized boolean hasFlag(BarFlag flag) {
        return flags.contains(flag);
    }

    @Override
    public synchronized void setProgress(double progress) {
        if (progress < 0.0 || progress > 1.0) {
            throw new IllegalArgumentException("Progress must be between 0.0 and 1.0 (" + progress + ")");
        }
        this.progress = progress;
        publish();
    }

    @Override
    public synchronized double getProgress() {
        return progress;
    }

    @Override
    public synchronized void addPlayer(Player player) {
        if (player == null) {
            throw new IllegalArgumentException("player == null");
        }
        if (player.isOnline() && players.add(player)) {
            publish();
        }
    }

    @Override
    public synchronized void removePlayer(Player player) {
        if (player == null) {
            throw new IllegalArgumentException("player == null");
        }
        if (players.remove(player)) {
            publish();
        }
    }

    @Override
    public synchronized void removeAll() {
        if (!players.isEmpty()) {
            players.clear();
            publish();
        }
    }

    @Override
    public synchronized List<Player> getPlayers() {
        return List.copyOf(players);
    }

    @Override
    public synchronized void setVisible(boolean visible) {
        if (this.visible != visible) {
            this.visible = visible;
            publish();
        }
    }

    @Override
    public synchronized boolean isVisible() {
        return visible;
    }

    @Override
    public void show() {
        setVisible(true);
    }

    @Override
    public void hide() {
        setVisible(false);
    }
}
