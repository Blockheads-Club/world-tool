// SPDX-License-Identifier: GPL-3.0-or-later
// Independent cross-check of the .ab encryption, written against the JDK's own crypto (SunJCE PBKDF2WithHmacSHA1,
// AES/CBC/PKCS5Padding, java.util.zip) following the steps of AOSP PerformAdbBackupTask / PerformAdbRestoreTask and
// PasswordUtils (re-implemented from their documented behaviour, no AOSP code copied).
//
//   java AbXcheck.java encrypt <password> <version> <tar-in> <ab-out>
//   java AbXcheck.java decrypt <password> <ab-in> <tar-out>
import java.io.*;
import java.nio.file.*;
import java.security.SecureRandom;
import java.util.Arrays;
import java.util.zip.*;
import javax.crypto.*;
import javax.crypto.spec.*;

public class AbXcheck {
    static final int ROUNDS = 10000;

    static SecretKey charKey(char[] pw, byte[] salt, int rounds) throws Exception {
        SecretKeyFactory kf = SecretKeyFactory.getInstance("PBKDF2WithHmacSHA1"); // UTF-8 encodes the chars
        return kf.generateSecret(new PBEKeySpec(pw, salt, rounds, 256));
    }

    // makeKeyChecksum: master key bytes -> (char) signed byte -> PBKDF2
    static byte[] checksum(byte[] mk, byte[] salt, int rounds) throws Exception {
        char[] c = new char[mk.length];
        for (int i = 0; i < mk.length; i++) c[i] = (char) mk[i];
        return charKey(c, salt, rounds).getEncoded();
    }

    static String hex(byte[] b) {
        StringBuilder s = new StringBuilder();
        for (byte x : b) s.append(String.format("%02X", x & 0xff));
        return s.toString();
    }

    static byte[] unhex(String s) {
        byte[] b = new byte[s.length() / 2];
        for (int i = 0; i < b.length; i++) b[i] = (byte) Integer.parseInt(s.substring(2 * i, 2 * i + 2), 16);
        return b;
    }

    static String line(InputStream in) throws IOException {
        StringBuilder s = new StringBuilder();
        int c;
        while ((c = in.read()) != '\n') { if (c < 0) throw new EOFException(); s.append((char) c); }
        return s.toString();
    }

    public static void main(String[] a) throws Exception {
        SecureRandom rng = new SecureRandom();
        if (a[0].equals("encrypt")) {
            String pw = a[1];
            byte[] tar = Files.readAllBytes(Path.of(a[3]));
            byte[] userSalt = new byte[64], ckSalt = new byte[64], mk = new byte[32];
            rng.nextBytes(userSalt); rng.nextBytes(ckSalt); rng.nextBytes(mk);
            SecretKey userKey = new SecretKeySpec(charKey(pw.toCharArray(), userSalt, ROUNDS).getEncoded(), "AES");
            Cipher c = Cipher.getInstance("AES/CBC/PKCS5Padding");
            c.init(Cipher.ENCRYPT_MODE, new SecretKeySpec(mk, "AES"));
            byte[] iv = c.getIV();
            Cipher mkc = Cipher.getInstance("AES/CBC/PKCS5Padding");
            mkc.init(Cipher.ENCRYPT_MODE, userKey);
            byte[] ck = checksum(mk, ckSalt, ROUNDS);
            ByteArrayOutputStream blob = new ByteArrayOutputStream();
            DataOutputStream d = new DataOutputStream(blob);
            d.writeByte(iv.length); d.write(iv); d.writeByte(mk.length); d.write(mk); d.writeByte(ck.length); d.write(ck);
            String header = "ANDROID BACKUP\n" + a[2] + "\n1\nAES-256\n" + hex(userSalt) + "\n" + hex(ckSalt) + "\n"
                + ROUNDS + "\n" + hex(mkc.getIV()) + "\n" + hex(mkc.doFinal(blob.toByteArray())) + "\n";
            try (OutputStream out = new FileOutputStream(a[4])) {
                out.write(header.getBytes("UTF-8"));
                CipherOutputStream co = new CipherOutputStream(out, c);
                DeflaterOutputStream z = new DeflaterOutputStream(co, new Deflater(Deflater.BEST_COMPRESSION), true);
                z.write(tar);
                z.close();
            }
        } else {
            try (InputStream in = new BufferedInputStream(new FileInputStream(a[2]))) {
                if (!line(in).equals("ANDROID BACKUP")) throw new IOException("magic");
                line(in);
                boolean compressed = line(in).equals("1");
                if (!line(in).equals("AES-256")) throw new IOException("not encrypted");
                byte[] userSalt = unhex(line(in)), ckSalt = unhex(line(in));
                int rounds = Integer.parseInt(line(in));
                byte[] userIv = unhex(line(in)), mkBlob = unhex(line(in));
                SecretKey userKey = new SecretKeySpec(charKey(a[1].toCharArray(), userSalt, rounds).getEncoded(), "AES");
                Cipher c = Cipher.getInstance("AES/CBC/PKCS5Padding");
                c.init(Cipher.DECRYPT_MODE, userKey, new IvParameterSpec(userIv));
                byte[] blob = c.doFinal(mkBlob);
                int off = 0, len = blob[off++];
                byte[] iv = Arrays.copyOfRange(blob, off, off + len); off += len; len = blob[off++];
                byte[] mk = Arrays.copyOfRange(blob, off, off + len); off += len; len = blob[off++];
                byte[] ck = Arrays.copyOfRange(blob, off, off + len);
                if (!Arrays.equals(ck, checksum(mk, ckSalt, rounds))) throw new IOException("checksum mismatch");
                Cipher dc = Cipher.getInstance("AES/CBC/PKCS5Padding");
                dc.init(Cipher.DECRYPT_MODE, new SecretKeySpec(mk, "AES"), new IvParameterSpec(iv));
                InputStream s = new CipherInputStream(in, dc);
                if (compressed) s = new InflaterInputStream(s);
                Files.write(Path.of(a[3]), s.readAllBytes());
            }
        }
    }
}
