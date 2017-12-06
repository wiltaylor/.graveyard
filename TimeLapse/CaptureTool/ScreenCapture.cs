using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.Windows.Forms;

namespace CaptureTool
{
    public class ScreenCapture
    {
        private readonly string _path;
        public ScreenCapture(string path)
        {
            _path = path;
        }

        public void Snapshot()
        {
            var timestamp = DateTime.Now.ToString("yyyyMMddHHmmss.fff");
            int index = 0;
            foreach (var s in Screen.AllScreens)
            {

                TakeScreenShot(s.Bounds.Left, s.Bounds.Top, s.Bounds.Width, s.Bounds.Height, $"{_path}\\{timestamp}.{index}.png", ImageFormat.Png);
                index++;
            }
        }

        private void TakeScreenShot(int left, int top, int width, int height, string path, ImageFormat format)
        {
            using (Bitmap bmp = new Bitmap(width, height))
            {
                using (Graphics g = Graphics.FromImage(bmp))
                {
                    g.CopyFromScreen(left, top, 0, 0, bmp.Size);
                }

                bmp.Save(path, format);
            }
        }
    }
}
