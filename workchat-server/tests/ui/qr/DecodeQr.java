// Decodes the QR code in a PNG with ZXing (the decoder the Android app uses) and prints its text.
//   java -cp core.jar:javase.jar tests/e2e/qr/DecodeQr.java image.png
import com.google.zxing.BinaryBitmap;
import com.google.zxing.MultiFormatReader;
import com.google.zxing.client.j2se.BufferedImageLuminanceSource;
import com.google.zxing.common.HybridBinarizer;
import java.io.File;
import javax.imageio.ImageIO;

public class DecodeQr {
    public static void main(String[] a) throws Exception {
        var img = ImageIO.read(new File(a[0]));
        var bmp = new BinaryBitmap(new HybridBinarizer(new BufferedImageLuminanceSource(img)));
        System.out.print(new MultiFormatReader().decode(bmp).getText());
    }
}
