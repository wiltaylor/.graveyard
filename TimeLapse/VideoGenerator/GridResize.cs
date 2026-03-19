using System;
using System.Drawing;
using System.Drawing.Drawing2D;
using System.Drawing.Imaging;
using System.IO;
using System.Linq;
using System.Text.RegularExpressions;

namespace VideoGenerator
{
    public class GridResize : IImageResizer
    {
        private string _sourcePath;
        private string _destPath;
        private int _width;
        private int _height;

        public void SetResolution(int width, int height)
        {
            _width = width;
            _height = height;
        }

        public void SetFolders(string source, string destination)
        {
            _sourcePath = source;
            _destPath = destination;
        }

        public void Process(int screenCount, int[] screenOrder)
        {
            var fileList = Directory.GetFiles(_sourcePath, "*.png").OrderBy(f => f);

            if(screenCount > 9)
                throw new ApplicationException("Can't support more than 9 screens at the moment.");

            var rows = 0;
            var cols = 0;

            if (screenCount > 7)
            {
                rows = 3;
                cols = 3;
            }
            else if (screenCount > 4)
            {
                rows = 2;
                cols = 3;
            }
            else if (screenCount > 2)
            {
                rows = 2;
                cols = 2;
            }
            else if (screenCount == 2)
            {
                rows = 1;
                cols = 2;
            }
            else
            {
                rows = 1;
                cols = 1;
            }

            var cellWidth = _width / cols;
            var cellHeight = _height / rows;

            var lastpattern = Guid.NewGuid().ToString(); //so it doesn't match anything.
            foreach (var file in fileList)
            {
                if(Regex.IsMatch(file, lastpattern))
                    continue;

                var match = Regex.Match(file, "([0-9]{14}.[0-9]{3}).*.png");

                if(!match.Success)
                    continue;

                lastpattern = match.Groups[1].Value;

                using (var bmp = new Bitmap(_width, _height))
                {
                    using (var gfx = Graphics.FromImage(bmp))
                    {
                        gfx.FillRectangle(Brushes.Black, new Rectangle(0, 0, _width, _height));
                        gfx.CompositingMode = CompositingMode.SourceCopy;
                        gfx.CompositingQuality = CompositingQuality.HighQuality;
                        gfx.InterpolationMode = InterpolationMode.HighQualityBicubic;
                        gfx.SmoothingMode = SmoothingMode.HighQuality;
                        gfx.PixelOffsetMode = PixelOffsetMode.HighQuality;

                        var screenIndex = 0;

                        for(var col = 0; col < cols; col++)
                        for (var row = 0; row < rows; row++)
                        {
                            var filename = $"{_sourcePath}\\{lastpattern}.{screenIndex}.png";

                            if (!File.Exists(filename)) continue;

                            var img = Image.FromFile(filename);
                            var rect = new Rectangle(cellWidth * col, cellHeight * row, cellWidth, cellHeight);

                            gfx.DrawImage(img, rect);
                            img.Dispose();
                            screenIndex++;
                        }
                    }

                    bmp.Save($"{_destPath}\\{lastpattern}.png", ImageFormat.Png);
                }
            }
        }
    }
}
