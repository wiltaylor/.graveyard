using System;
using System.Collections.Generic;
using System.Diagnostics;
using System.Linq;
using System.Threading;
using CommandLineParser;
using FlexibleConfigEngine.Core.IOC;

namespace FlexibleConfigEngine
{
    class Program
    {
        static void Main(string[] args)
        {


#if DEBUG
            var skipPressEnter = false;
            if (args.FirstOrDefault() == "--skipdebugger")
            {
                args = args.Skip(1).ToArray();
                skipPressEnter = true;
            }
            else
            {
                if (args.Length == 0)
                {
                    Environment.CurrentDirectory = "E:\\configtest";


                    Console.WriteLine("Current command line: " + string.Join(" ", args));

                    Console.WriteLine("Type command line to change to or press enter to use existing one.");
                    Console.Write(">");

                    var newargs = Console.ReadLine();

                    if (!string.IsNullOrEmpty(newargs))
                        args = ParseCommandString(newargs);
                }
                else
                {
                    Console.WriteLine("Waiting for Debugger to be attached...Press D to continue without debugger");
                    while (!Debugger.IsAttached)
                    {
                        if (Console.KeyAvailable && Console.ReadKey(true).Key == ConsoleKey.D)
                            break;

                        Thread.Sleep(100);
                    }

                    Console.WriteLine(Debugger.IsAttached ? "Debugger attached..." : "Continuing without Debugger");
                }
            }
#else
            if (args.FirstOrDefault() == "--skipdebugger")
            {
                args = args.Skip(1).ToArray();
                Console.WriteLine("Stripped --skipdebugger from Release version...");
            }
#endif
            foreach (var line in new Bootstrap().StarT<ICommandParser>().Process(args))
                Console.WriteLine(line);


#if DEBUG
            if (!skipPressEnter)
            {
                Console.WriteLine("Press any key to exit");
                Console.Read();
            }
#endif
        }

#if DEBUG
        static string[] ParseCommandString(string command)
        {
            var result = new List<string>();
            var currentparts = "";
            var inQuotes = false;

            for (var i = 0; i < command.Length; i++)
            {
                if (!inQuotes && command.Substring(i, 1) == " ")
                {
                    if (currentparts == "")
                        continue;

                    result.Add(currentparts);
                    currentparts = "";
                    continue;
                }

                if (command.Substring(i, 1) == "\"")
                {
                    if (inQuotes)
                    {
                        result.Add(currentparts);
                        currentparts = "";
                        inQuotes = false;
                        continue;
                    }

                    inQuotes = true;
                    continue;
                }

                currentparts += command.Substring(i, 1);
            }

            if (currentparts != "")
                result.Add(currentparts);

            return result.ToArray();
        }


#endif
    }
}
