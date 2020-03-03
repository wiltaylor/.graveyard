using System.Collections.Generic;
using PetStoreCLI.Model;

namespace PetStoreCLI
{
    public interface IPetRepository
    {
        IEnumerable<Pet> GetAvailablePets();
    }
}