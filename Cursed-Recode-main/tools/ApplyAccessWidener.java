import net.fabricmc.loader.impl.lib.classtweaker.api.ClassTweaker;
import net.fabricmc.loader.impl.lib.classtweaker.api.ClassTweakerReader;
import org.objectweb.asm.ClassReader;
import org.objectweb.asm.ClassVisitor;
import org.objectweb.asm.ClassWriter;
import org.objectweb.asm.Opcodes;

import java.io.BufferedReader;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.nio.file.attribute.FileTime;
import java.util.Set;
import java.util.jar.Manifest;
import java.util.zip.ZipEntry;
import java.util.zip.ZipInputStream;
import java.util.zip.ZipOutputStream;

/**
 * Применяет access widener к jar игры — то же, что Loom делает при сборке, иначе javac и IDE
 * не видят «расширенные» поля/методы (в самой игре их расширяет Fabric Loader при загрузке классов).
 * Используется class-tweaker, который лежит внутри fabric-loader (тот же код, что и в рантайме).
 *
 * Запуск без компиляции (java source launcher), см. tools\aw.cmd:
 *   java -cp "libs\fabric\*" tools\ApplyAccessWidener.java <vanilla.jar> <файл.accesswidener> <out.jar>
 * Если out.jar новее и vanilla.jar, и .accesswidener — ничего не делает.
 */
public final class ApplyAccessWidener {
    public static void main(String[] args) throws Exception {
        if (args.length != 3) {
            System.err.println("usage: ApplyAccessWidener <vanilla.jar> <file.accesswidener> <out.jar>");
            System.exit(2);
        }
        Path vanilla = Path.of(args[0]), awFile = Path.of(args[1]), out = Path.of(args[2]);

        if (Files.exists(out)) {
            FileTime o = Files.getLastModifiedTime(out);
            if (o.compareTo(Files.getLastModifiedTime(vanilla)) >= 0 && o.compareTo(Files.getLastModifiedTime(awFile)) >= 0) {
                System.out.println("[aw] " + out.getFileName() + " up to date");
                return;
            }
        }

        ClassTweaker tweaker = ClassTweaker.newInstance();
        try (BufferedReader r = Files.newBufferedReader(awFile)) {
            // "official": 26.x без обфускации, рантайм-пространство имён у лоудера именно такое
            ClassTweakerReader.create(tweaker).read(r, "official");
        }
        Set<String> targets = tweaker.getTargets(); // внутренние имена классов, через '/'
        System.out.println("[aw] classes to widen: " + targets.size());

        Path tmp = out.resolveSibling(out.getFileName() + ".tmp");
        int changed = 0;
        try (ZipInputStream in = new ZipInputStream(Files.newInputStream(vanilla));
             ZipOutputStream zo = new ZipOutputStream(Files.newOutputStream(tmp))) {
            zo.setLevel(1);
            ZipEntry e;
            byte[] buf = new byte[1 << 16];
            while ((e = in.getNextEntry()) != null) {
                String name = e.getName();
                // Jar игры подписан Mojang: расширенный класс уже не совпадает с подписью, и JVM отказывается его
                // грузить (SecurityException: SHA-384 digest error). Подписи снимаем, из манифеста — хэши файлов.
                if (name.startsWith("META-INF/") && (name.endsWith(".SF") || name.endsWith(".RSA") || name.endsWith(".DSA") || name.endsWith(".EC"))) {
                    continue;
                }
                ZipEntry ne = new ZipEntry(name);
                ne.setTime(e.getTime());
                zo.putNextEntry(ne);
                if (name.equals("META-INF/MANIFEST.MF")) {
                    Manifest m = new Manifest(in);
                    m.getEntries().clear();
                    m.write(zo);
                } else if (name.endsWith(".class") && targets.contains(name.substring(0, name.length() - 6))) {
                    ClassReader cr = new ClassReader(in.readAllBytes());
                    ClassWriter cw = new ClassWriter(0);
                    ClassVisitor cv = tweaker.createClassVisitor(Opcodes.ASM9, cw, null);
                    cr.accept(cv, 0);
                    zo.write(cw.toByteArray());
                    changed++;
                } else {
                    copy(in, zo, buf);
                }
                zo.closeEntry();
            }
        }
        Files.move(tmp, out, StandardCopyOption.REPLACE_EXISTING);
        System.out.println("[aw] written " + out + " (" + changed + " classes widened)");
    }

    private static void copy(InputStream in, OutputStream out, byte[] buf) throws Exception {
        int n;
        while ((n = in.read(buf)) > 0) out.write(buf, 0, n);
    }
}
