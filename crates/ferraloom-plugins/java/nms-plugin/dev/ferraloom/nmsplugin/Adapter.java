package dev.ferraloom.nmsplugin;

import net.minecraft.server.MinecraftServer;

/** The version adapter: compiled against the server's own classes (build.py's PROVIDED stands in for them). */
final class Adapter {
    private Adapter() {
    }

    static String server() {
        return String.valueOf(MinecraftServer.getServer());
    }
}
