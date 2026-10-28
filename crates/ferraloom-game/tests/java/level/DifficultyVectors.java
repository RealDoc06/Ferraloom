import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.world.Difficulty;
import net.minecraft.world.DifficultyInstance;

/** Captures actual DifficultyInstance float bits at age/local/moon boundaries. */
public final class DifficultyVectors {
    public static void main(String[] args) throws Exception {
        var rows = new StringBuilder();
        for (Difficulty difficulty : Difficulty.values())
            for (long total : new long[]{Long.MIN_VALUE, 0, 72000, 72001, 1512000, Long.MAX_VALUE})
                for (long local : new long[]{-1, 0, 1, 3600000, Long.MAX_VALUE})
                    for (float moon : new float[]{-1.0F, 0.0F, 0.25F, 0.5F, 1.0F}) {
                        var value = new DifficultyInstance(difficulty, total, local, moon);
                        rows.append(difficulty.getId()).append('\t').append(total).append('\t').append(local).append('\t')
                            .append(Integer.toUnsignedString(Float.floatToRawIntBits(moon))).append('\t')
                            .append(Integer.toUnsignedString(Float.floatToRawIntBits(value.getEffectiveDifficulty()))).append('\t')
                            .append(Integer.toUnsignedString(Float.floatToRawIntBits(value.getSpecialMultiplier()))).append('\t')
                            .append(value.isHard()).append('\t').append(value.isHarderThan(3.0F)).append('\n');
                    }
        Files.writeString(Path.of(args[0]), rows.toString());
    }
}
