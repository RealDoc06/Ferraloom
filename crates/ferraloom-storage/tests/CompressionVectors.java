import java.io.*;
import java.nio.file.*;
import net.minecraft.world.level.chunk.storage.RegionFileVersion;

/** Independent vanilla stream encoder/decoder for Java LZ4Block compatibility. */
public class CompressionVectors {
    public static void main(String[] args) throws Exception {
        Path dir = Path.of(args[1]);
        Files.createDirectories(dir);
        byte[] bytes = new byte[140000];
        int random = 12345;
        for (int i = 0; i < bytes.length; i++) {
            random ^= random << 13; random ^= random >>> 17; random ^= random << 5;
            bytes[i] = i < 65536 ? (byte) (i % 71) : (byte) random;
        }
        for (int id = 1; id <= 4; id++) {
            Path file = dir.resolve("compression-" + id + ".bin");
            RegionFileVersion version = RegionFileVersion.fromId(id);
            if (args[0].equals("generate")) {
                try (OutputStream out = version.wrap(Files.newOutputStream(file))) {
                    out.write(bytes);
                }
            } else {
                try (InputStream in = version.wrap(Files.newInputStream(file))) {
                    if (!java.util.Arrays.equals(bytes, in.readAllBytes())) {
                        throw new AssertionError("vanilla rejected compression " + id);
                    }
                }
            }
        }
    }
}
