using System;
using System.IO;
using System.Linq;

namespace PetStoreCLI
{
    class Program
    {
        /// <summary>
        /// Application Entry Point
        /// </summary>
        /// <param name="args">Arguments passed in from the command line. See Usage for available command line arguments.</param>
        static void Main(string[] args)
        {
            //I figured using a DI container would be overkill for this example.
            var petRepo = new PetRepository("https://petstore.swagger.io/v2");
            var petService = new PetService(petRepo);
            
            if (args.Length > 1)
            {
                Usage();
                return;
            }

            if (args.Length == 1)
            {
                if (args[0] == "--clean")
                {
                    petService.CleanJunkNames = true;
                }
                else
                {
                    Usage();
                    return;
                }
            }

            try
            {
                foreach (var cat in petService.GetAvailablePets())
                {
                    Console.WriteLine($"[{cat.Key}]");

                    foreach (var pet in cat.Value)
                    {
                        Console.WriteLine($"  {pet.Name}");
                    }
                }
            }
            catch (Exception e)
            {
                Console.WriteLine("There was a problem connecting to the Pet service. Please check that you are connected to the internet.");
                
                //Dump last error into text file in temp.
                //Normally would use a log file but trying to keep this sample simple.
                File.WriteAllText(Environment.ExpandEnvironmentVariables("%temp%\\petstorecli_err.txt"),  e.ToString());
            #if DEBUG
                Console.WriteLine(e);
            #endif

            }
        }

        /// <summary>
        /// Prints available command line arguments to the user.
        /// This will be displayed if the user types unexpected arguments.
        /// </summary>
        static void Usage()
        {
            Console.WriteLine("Usage:");
            Console.WriteLine("PetStoreCLI.exe [--clean]");
            Console.WriteLine("");
            Console.WriteLine("--Clean \t This will mark categories with corrupted characters as No Category.");
            Console.WriteLine("");
        }


    }
}
