using System;

namespace AIEDemo.Winforms
{

    /// <summary>
    /// Available chareceter classes
    /// </summary>
    public enum CharecterClass
    {
        Fighter,
        MagicUser
    }

    /// <summary>
    /// Sex of the charecter.
    /// </summary>
    public enum CharecterSex
    {
        Male,
        Female,
        Other
    }

    /// <summary>
    /// Simple character class that is used for serialisation.
    /// </summary>
    [Serializable]
    public class Charecter
    {
        /// <summary>
        /// Name of the character.
        /// </summary>
        public string Name { get; set; }

        /// <summary>
        /// Character class of the player. Think their profession.
        /// </summary>
        public CharecterClass Class { get; set; }

        /// <summary>
        /// The sex of the character.
        /// </summary>
        public CharecterSex Sex { get; set; }

        /// <summary>
        /// How strong the character is. This value determines how much physical damage a player can do.
        /// </summary>
        public int Strength { get; set; }

        /// <summary>
        /// The constitution of the character. This value determines how much HP a character has and how resistant to diseases they are.
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

        /// <summary>
        /// HP of the character is how much damage they can take before dying.
        /// </summary>
        public int Hp => 10 + (Constitution * 10);

        /// <summary>
        /// Mana of the character. How many points are available for casing spells.
        /// </summary>
        public int Mana => Intelligence * 10;

        /// <summary>
        /// Retrieves a characters dice roll stats.
        /// </summary>
        /// <returns>A dice roll object containing the same stats as the character.</returns>
        public DiceRoll GetDiceRoll()
        {
            return new DiceRoll
            {
                Strength = Strength,
                Constitution = Constitution,
                Dexterity = Dexterity,
                Intelligence = Intelligence,
                Wisdom = Wisdom,
                Charisma = Charisma
            };
        }

        /// <summary>
        /// Set the stats from a character from dice rolls.
        /// </summary>
        /// <param name="dice">Dice roll object to set charecter stats from.</param>
        public void SetFromDiceRoll(DiceRoll dice)
        {
            Strength = dice.Strength;
            Constitution = dice.Constitution;
            Dexterity = dice.Dexterity;
            Intelligence = dice.Intelligence;
            Wisdom = dice.Wisdom;
            Charisma = dice.Charisma;
        }
    }
}
