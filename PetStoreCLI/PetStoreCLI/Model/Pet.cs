using System;
using System.Collections.Generic;
using System.Text;

namespace PetStoreCLI.Model
{
    public class Pet
    {
        public long Id { get; set; }
        public string Name { get; set; }
        public string[] PhotoUrls { get; set; }
        public Tag[] Tags { get; set; }
        public string Status { get; set; }
        public Category Category { get; set; }
    }
}
