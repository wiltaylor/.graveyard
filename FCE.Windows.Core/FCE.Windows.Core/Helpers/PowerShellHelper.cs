using System;
using System.Diagnostics;
using System.Text;
using System.Text.RegularExpressions;
using Newtonsoft.Json;

namespace FCE.Windows.Core.Helpers
{
    public static class PowerShellHelper
    {
        public static string Run(string script)
        {
            return Run(new[] {script});
        }
        public static string Run(string[] script)
        {
            var tempFilePath = $"{Environment.GetEnvironmentVariable("TEMP")}\\{Guid.NewGuid()}.ps1";

            System.IO.File.WriteAllLines(tempFilePath, script);

            var startinfo = new ProcessStartInfo
            {
                FileName = "powershell.exe",
                Arguments = $"-executionpolicy bypass -noninteractive -noprofile -file {tempFilePath}",
                RedirectStandardError = true,
                RedirectStandardOutput = true
            };

            var process = Process.Start(startinfo);
            var stderr = new StringBuilder();
            var stdout = new StringBuilder();

            while (!process.HasExited)
            {
                while (!process.StandardOutput.EndOfStream)
                {
                    stdout.AppendLine(process.StandardOutput.ReadLine());
                }

                while (!process.StandardError.EndOfStream)
                {
                    stderr.AppendLine(process.StandardError.ReadLine());
                }
            }

            while (!process.StandardOutput.EndOfStream)
            {
                stdout.AppendLine(process.StandardOutput.ReadLine());
            }

            while (!process.StandardError.EndOfStream)

            {
                stderr.AppendLine(process.StandardError.ReadLine());
            }

            stdout.AppendLine("errors:").Append(stderr);

            try
            {
                System.IO.File.Delete(tempFilePath);
            }
            catch
            {
                //ignore if can't remove file.
            }

            return stdout.ToString();
        }

        public static T ExtractJson<T>(string lines)
        {
            var sb = new StringBuilder();
            var reading = false;

            foreach (var line in lines.Split(Environment.NewLine.ToCharArray()))
            {
                if (line.Contains("---end json---"))
                    reading = false;

                if (reading)
                    sb.AppendLine(line);

                if (line.Contains("---start json---"))
                    reading = true;
            }

            return JsonConvert.DeserializeObject<T>(sb.ToString());
        }
    }
}
