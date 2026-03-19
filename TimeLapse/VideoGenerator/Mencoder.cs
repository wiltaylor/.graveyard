using System;
using System.Diagnostics;

namespace VideoGenerator
{
    public class Mencoder
    {
        public void GenerateVideo(string source, string destination, int width, int height, int fps)
        {

            var bitrate = 50 * fps * width * height / 256;

            var startinfo = new ProcessStartInfo
            {
                FileName = AppDomain.CurrentDomain.BaseDirectory + "\\mencoder\\mencoder.exe",
                Arguments =
                    $"mf://*.png -mf w={width}:h={height}:fps={fps}:type=png -ovc lavc -lavcopts vcodec=mpeg4:mbd=2:trell:vbitrate={bitrate} copy -o \"{destination}\"",
                WorkingDirectory = source
                
            };

            var proc = Process.Start(startinfo);

            proc?.WaitForExit();
        }

        public void AddAudio(string source, string destination, string audioFile)
        {
            var startinfo = new ProcessStartInfo
            {
                FileName = AppDomain.CurrentDomain.BaseDirectory + "\\mencoder\\mencoder.exe",
                Arguments =
                    $"-ovc copy -audiofile \"{audioFile}\" -oac copy \"{source}\" -o \"{destination}\"",
            };

            var proc = Process.Start(startinfo);

            proc.WaitForExit();
        }
    }
}
