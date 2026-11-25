package dev.ferraloom.bukkit;

import dev.ferraloom.bukkit.stub.StubWorldBorder;
import java.util.concurrent.TimeUnit;
import org.bukkit.Location;
import org.bukkit.World;

/** A world's border (CraftWorldBorder), read from the server when asked. */
final class FerraloomWorldBorder extends StubWorldBorder {
    /** WorldBorder.MAX_SIZE and MAX_CENTER_COORDINATE. */
    private static final double MAX_SIZE = 5.999997E7;
    private static final double MAX_CENTER = 2.9999984E7;
    private final FerraloomWorld world;

    FerraloomWorldBorder(FerraloomWorld world) {
        this.world = world;
    }

    private double[] read() {
        return WorldNative.border(world.index);
    }

    private void change(int what, double a, long b) {
        WorldNative.change(world.index, what, Double.doubleToRawLongBits(a), b, 0, 0);
    }

    @Override
    public World getWorld() {
        return world;
    }

    /** CraftWorldBorder.reset: vanilla's defaults. */
    @Override
    public void reset() {
        setSize(MAX_SIZE);
        setDamageAmount(0.2);
        setDamageBuffer(5.0);
        setWarningDistance(5);
        setWarningTime(15);
        setCenter(0, 0);
    }

    @Override
    public double getSize() {
        return read()[2];
    }

    @Override
    public void setSize(double size) {
        setSize(size, 0L);
    }

    @Override
    public void setSize(double size, long seconds) {
        setSize(Math.min(MAX_SIZE, Math.max(1.0, size)), TimeUnit.SECONDS, Math.max(0L, seconds));
    }

    @Override
    public void setSize(double size, TimeUnit unit, long time) {
        if (size < 1.0 || size > MAX_SIZE) {
            throw new IllegalArgumentException("size must be between 1.0D and " + MAX_SIZE);
        }
        if (time < 0) {
            throw new IllegalArgumentException("time cannot be lower than 0");
        }
        change(WorldNative.BORDER_SIZE, size, unit.toMillis(time) / 50);
    }

    @Override
    public Location getCenter() {
        double[] b = read();
        return new Location(world, b[0], 0, b[1]);
    }

    @Override
    public void setCenter(double x, double z) {
        if (Math.abs(x) > MAX_CENTER || Math.abs(z) > MAX_CENTER) {
            throw new IllegalArgumentException("center must be within " + MAX_CENTER);
        }
        WorldNative.change(world.index, WorldNative.BORDER_CENTER, Double.doubleToRawLongBits(x),
                Double.doubleToRawLongBits(z), 0, 0);
    }

    @Override
    public void setCenter(Location location) {
        setCenter(location.getX(), location.getZ());
    }

    @Override
    public double getDamageBuffer() {
        return read()[3];
    }

    @Override
    public void setDamageBuffer(double blocks) {
        change(WorldNative.BORDER_DAMAGE_BUFFER, blocks, 0);
    }

    @Override
    public double getDamageAmount() {
        return read()[4];
    }

    @Override
    public void setDamageAmount(double damage) {
        change(WorldNative.BORDER_DAMAGE_AMOUNT, damage, 0);
    }

    @Override
    public int getWarningTime() {
        return (int) read()[6];
    }

    @Override
    public void setWarningTime(int seconds) {
        WorldNative.change(world.index, WorldNative.BORDER_WARNING_TIME, seconds, 0, 0, 0);
    }

    @Override
    public int getWarningDistance() {
        return (int) read()[5];
    }

    @Override
    public void setWarningDistance(int distance) {
        WorldNative.change(world.index, WorldNative.BORDER_WARNING_DISTANCE, distance, 0, 0, 0);
    }

    /** CraftWorldBorder.isInside: the location's world, within the square (WorldBorder.isWithinBounds). */
    @Override
    public boolean isInside(Location location) {
        if (location.getWorld() != null && !world.equals(location.getWorld())) {
            return false;
        }
        double[] b = read();
        double half = b[2] / 2;
        double x = location.getX();
        double z = location.getZ();
        return x >= b[0] - half && x < b[0] + half && z >= b[1] - half && z < b[1] + half;
    }

    @Override
    public double getMaxSize() {
        return MAX_SIZE;
    }

    @Override
    public double getMaxCenterCoordinate() {
        return MAX_CENTER;
    }
}
