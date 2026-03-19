using System;
using System.Drawing;
using System.Drawing.Imaging;
using System.IO;
using System.Linq;
using System.Windows.Forms;

namespace ChronoConverter
{
    public partial class frmMain : Form
    {
        public frmMain()
        {
            InitializeComponent();
        }

        private void label4_Click(object sender, EventArgs e)
        {

        }

        private void BrowseForFolder(string text, TextBox textbox, bool showNewFolder)
        {
            var openFolder = new FolderBrowserDialog
            {
                Description = text,
                ShowNewFolderButton = showNewFolder
            };

            if (openFolder.ShowDialog() != DialogResult.OK)
                return;

            textbox.Text = openFolder.SelectedPath;
        }

        private void button1_Click(object sender, EventArgs e)
        {
            var fileList = Directory.GetFiles(txtSource.Text, "*.png").OrderBy(File.GetLastWriteTime).ToArray();
            var screenCount = int.Parse(txtScreenCount.Text);
            var screenWidth = int.Parse(txtWidth.Text);
            var screenHeight = int.Parse(txtHeight.Text);

            progbar.Maximum = fileList.Length;
            progbar.Minimum = 0;
            progbar.Value = 0;

            foreach (var file in fileList)
            {
                var timestamp = DateTime.Now.ToString("yyyyMMddHHmmss.fff");


                using (var srcbmp = Image.FromFile(file))
                {
                    for (var i = 0; i < screenCount; i++)
                    {
                        var bmp = new Bitmap(screenWidth, screenHeight);

                        using (var gfx = Graphics.FromImage(bmp))
                        {
                            gfx.FillRectangle(Brushes.Black, new Rectangle(0, 0, screenWidth, screenHeight));

                            var srcRect = new Rectangle(screenWidth * i, 0, screenWidth, screenHeight);
                            var dstRect = new Rectangle(0, 0, screenWidth, screenHeight);

                            gfx.DrawImage(srcbmp, dstRect, srcRect, GraphicsUnit.Pixel);
                        }

                        bmp.Save($"{txtDest.Text}\\{timestamp}.{i}.png", ImageFormat.Png);
                        bmp.Dispose();
                        Application.DoEvents();
                    }
                }

                progbar.Value++;
            }

            MessageBox.Show("Done");

        }

        private void btnSource_Click(object sender, EventArgs e)
        {
            BrowseForFolder("Select source folder", txtSource, false);
        }

        private void btnDest_Click(object sender, EventArgs e)
        {
            BrowseForFolder("Select destination folder", txtDest, true);
        }
    }
}
