using System.Collections.Generic;
using System.Linq;
using PetStoreCLI.Model;

namespace PetStoreCLI
{
    /// <summary>
    /// Pet service sorts data retrieved from the Pet Repository into a format we can use to display.
    /// This an example business logic layer.
    /// </summary>
    public class PetService
    {
        private readonly IPetRepository _petRepository;

        /// <summary>
        /// Set this to true to filter out Categories with corrupt names.
        /// A corrupt name is name containing characters less than ascii 32 or greater than 256.
        /// </summary>
        public bool CleanJunkNames { get; set; } = true;

        /// <summary>
        /// Constructor
        /// </summary>
        /// <param name="petRepository">Pet repository to retrieve pets from.</param>
        public PetService(IPetRepository petRepository)
        {
            _petRepository = petRepository;
        }

        /// <summary>
        /// Get sorted dictionary containing all Categories as keys and pets as values.
        /// Pets are sorted into descending order.
        /// </summary>
        /// <returns>Pet Category dictionary</returns>
        public Dictionary<string, Pet[]> GetAvailablePets()
        {
            //Removing pets which don't have a name. The spec says pets must have a name but server doesn't appear to validate this.
            var allPets = _petRepository.GetAvailablePets().Where(p => !string.IsNullOrEmpty(p.Name));

            //Remove any entry with invalid characters in its name.
            if (CleanJunkNames)
                allPets = allPets.Where(p => !p.Name.Any(t => t < 32 || t > 254));

            //Sort pets by category and by name in descending order.
            var sorted = allPets.GroupBy(r => r.Category != null ?
                CleanName(r.Category.Name) :
                "No Category").OrderBy(c => c.Key);

            return sorted.ToDictionary(g => g.Key, g => g.OrderByDescending(p => p.Name).ToArray());
        }

        /// <summary>
        /// Cleans up names of Categories.
        /// If a Category is null or an empty string replace it with No Category.
        /// Will also return No Catagory for junk names if CleanJunkNames is set.
        /// </summary>
        /// <param name="name">Name to clean</param>
        /// <returns>Cleaned name</returns>
        private string CleanName(string name)
        {
            if (string.IsNullOrEmpty(name))
                return "No Category";

            if (CleanJunkNames && name.Any(t => t < 32 || t > 254))
                return "No Category";

            return name;
        }
    }
}
