using System.Collections.Generic;
using PetStoreCLI.Model;
using Xunit;

namespace PetStoreCLI.UnitTest
{
    public class PetService_Test
    {
        public class FakeRepository : IPetRepository
        {
            public IEnumerable<Pet> PetData;

            public IEnumerable<Pet> GetAvailablePets()
            {
                return PetData;
            }
        }

        [Fact]
        public void When_CallingGetAvailablePets_Should_ConvertNullCategoriesToNoCategory()
        {
            //Arrange
            var petData = new FakeRepository();
            petData.PetData = new[]
            {
                new Pet
                {
                    Name = "TestPet",
                    Category = null
                }
            };

            var sut = new PetService(petData);

            //Act
            var result = sut.GetAvailablePets();

            //Assert
            Assert.True(result.ContainsKey("No Category"));
        }

        [Fact]
        public void When_CallingGet_AvailablePets_Should_SortPetNamesInDescendingOrder()
        {
            //Arrange
            var petData = new FakeRepository();
            petData.PetData = new[]
            {
                new Pet
                {
                    Name = "ABC",
                    Category = new Category
                    {
                        Id = 0,
                        Name = "Cat1"
                    }
                },
                new Pet
                {
                    Name = "DEF",
                    Category = new Category
                    {
                        Id = 0,
                        Name = "Cat1"
                    }
                }
            };

            var sut = new PetService(petData);

            //Act
            var result = sut.GetAvailablePets();

            //Assert
            Assert.True(result["Cat1"][0].Name == "DEF");
            Assert.True(result["Cat1"][1].Name == "ABC");
        }

    }
}
