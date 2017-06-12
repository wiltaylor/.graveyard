using System;

namespace AIEDemo.Winforms
{
    /// <summary>
    /// Simple class that holds a dice roll for charecter stats.
    /// </summary>
    public class DiceRoll
    {
        /// <summary>
        /// Constructor creates a new dice roll with random dice roll.
        /// </summary>
        public DiceRoll()
        {
            var rand = new Random();
            Strength = rand.Next(3, 18);
            Constitution = rand.Next(3, 18);
            Dexterity = rand.Next(3, 18);
            Intelligence = rand.Next(3, 18);
            Wisdom = rand.Next(3, 18);
            Charisma = rand.Next(3, 18);
        }

        /// <summary>
        /// Clears a dice roll to 0 values and clears name.
        /// </summary>
        public void Clear()
        {
            Strength = 0;
            Constitution = 0;
            Dexterity = 0;
            Intelligence = 0;
            Wisdom = 0;
            Charisma = 0;
            Name = "";
        }

        /// <summary>
        /// Name of dice roll. Used for saving and retriving rolls.
        /// </summary>
        public string Name { get; set; }

        /// <summary>
        /// The constitution of the character. This value determines how much HP a character has and how resistant to diseases they are.
        /// </summary>
        public int Strength { get; set; }

        /// <summary>
        /// The dexterity of the character. This determines how well the character can dodge and use Dexterity based weapons.
        /// </summary>
        public int Constitution { get; set; }
        /// <summary>
        /// The dexterity of the character. This determines how well the character can dodge and use Dexterity based weapons.
        /// </summary>
        public int Dexterity { get; set; }

        /// <summary>
        /// The intelligence of the character. This determines how much mana the character has and what types of spells they can learn.
        /// </summary>
        public int Intelligence { get; set; }

        /// <summary>
        /// The Wisdom of the character. This determines the initiative of the character in battle (higher the value the sooner it is their turn).
        /// </summary>
        public int Wisdom { get; set; }

        /// <summary>
        /// The Charisma of the character. This determines how well the character does during social interactions. High values allow for persuade checks.
        /// </summary>
        public int Charisma { get; set; }
    }
}
