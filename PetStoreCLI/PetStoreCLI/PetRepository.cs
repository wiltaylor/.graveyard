using System;
using System.Collections.Generic;
using System.Net.Http;
using Newtonsoft.Json;
using PetStoreCLI.Model;

namespace PetStoreCLI
{
    /// <summary>
    /// This is the example data layer of the application. All code that touches the infrastructure goes here.
    /// This is so the business layer can be easily unit tested.
    /// </summary>
    public class PetRepository : IPetRepository
    {
        private readonly string _baseUrl;

        /// <summary>
        /// Constructor
        /// </summary>
        /// <param name="url">Base url for the web service. Usually in format of https://hostname/v2 </param>
        public PetRepository(string url)
        {
            _baseUrl = url;
        }

        /// <summary>
        /// Get all pets that have the status of Available.
        /// </summary>
        /// <returns>IEnumerable of Pets with status of Available.</returns>
        public IEnumerable<Pet> GetAvailablePets()
        {
            var client = new HttpClient();
            var task = client.GetStringAsync($"{_baseUrl}/pet/findByStatus?status=available");

            task.Wait();

            if(task.IsCompletedSuccessfully)
                return JsonConvert.DeserializeObject<Pet[]>(task.Result);
            else
                throw new ApplicationException("Failed to retrieve data from web service.");


        }
    }
}
