using System;
using System.Collections.Generic;
using System.IO;
using System.Linq;
using System.Windows.Forms;
using Newtonsoft.Json;

namespace AIEDemo.Winforms
{
    /// <summary>
    /// Roll Save/Load form. This form allows the user to save and load rolls.
    /// 
    /// This form runs in 2 modes Save and Load.
    /// 
    /// In Save mode a text box is shown to allow the user to store a roll and in Load mode the user
    /// has a drop down where they can select previously saved rolls.
    /// </summary>
    public partial class frmRoll : Form
    {
        /// <summary>
        /// If ShowLoad is called this contains the selected dice roll.
        /// </summary>
        public DiceRoll Roll { get; set; }

        /// <summary>
        /// Returns true if cancel button was pressed.
        /// </summary>
        public bool WasCancelled { get; set; }

        // Private variables.
        private bool _saveMode;
        private bool _closedAsExpected;
        private List<DiceRoll> _rollList;

        /// <summary>
        /// Constructor
        /// </summary>
        public frmRoll()
        {
            InitializeComponent();
        }

        /// <summary>
        /// This method updates all the controlls on the form to match what is in the current selected Roll.
        /// </summary>
        private void RefreshView()
        {
            txtStr.Text = Roll.Strength.ToString();
            txtConst.Text = Roll.Constitution.ToString();
            txtDex.Text = Roll.Dexterity.ToString();
            txtInt.Text = Roll.Intelligence.ToString();
            txtWis.Text = Roll.Wisdom.ToString();
            txtChar.Text = Roll.Charisma.ToString();

            cmbRollName.Text = Roll.Name;
        }

        /// <summary>
        /// Shows the roll form in Save configuration.
        /// </summary>
        /// <param name="roll">Roll to save.</param>
        public void ShowSave(DiceRoll roll)
        {
            LoadRolls();
            Roll = roll;
            _saveMode = true;
            this.Text = "Save Roll";

            txtName.Visible = true;
            cmbRollName.Visible = false;
            btnOk.Enabled = false;
            btnRemove.Visible = false;

            RefreshView();
            ShowDialog();
        }

        /// <summary>
        /// Shows the roll form in Load configuration.
        /// </summary>
        public void ShowLoad()
        {
            LoadRolls();
            Roll = new DiceRoll();
            Roll.Clear();
            _saveMode = false;
            this.Text = "Load Roll";

            txtName.Visible = false;
            cmbRollName.Visible = true;
            btnOk.Enabled = false;
            btnRemove.Visible = true;
            btnRemove.Enabled = false;

            RefreshView();
            ShowDialog();
        }

        /// <summary>
        /// Event handler for when the form is closing.
        /// Will set WasCancelled to true if the user closed the form with the x button.
        /// </summary>
        private void frmRoll_FormClosing(object sender, FormClosingEventArgs e)
        {
            if (e.CloseReason == CloseReason.UserClosing && !_closedAsExpected)
                WasCancelled = true;
        }

        /// <summary>
        /// Handler for roll name text box.
        /// </summary>
        private void txtName_TextChanged(object sender, EventArgs e)
        {
            if (_saveMode)
            {
                btnOk.Enabled = txtName.Text != string.Empty;
                btnRemove.Enabled = txtName.Text != string.Empty;
                Roll.Name = txtName.Text;
            }
        }

        /// <summary>
        /// Stores rolls to disk.
        /// </summary>
        private void SaveRolls()
        {
            File.WriteAllText("Rolls.json", JsonConvert.SerializeObject(_rollList, Formatting.Indented));
        }

        /// <summary>
        /// Loads rolls from disk.
        /// </summary>
        private void LoadRolls()
        {
            _rollList = !File.Exists("Rolls.json") ? new List<DiceRoll>() : JsonConvert.DeserializeObject<List<DiceRoll>>(File.ReadAllText("Rolls.json"));

            cmbRollName.Items.Clear();

            foreach (var roll in _rollList)
                cmbRollName.Items.Add(roll.Name);
        }

        /// <summary>
        /// Event handler for cancel button.
        /// </summary>
        /// <param name="sender"></param>
        /// <param name="e"></param>
        private void btnCancel_Click(object sender, EventArgs e)
        {
            WasCancelled = true;
            Close();
        }

        /// <summary>
        /// Event handler for ok button.
        /// </summary>
        private void btnOk_Click(object sender, EventArgs e)
        {
            WasCancelled = false;
            _closedAsExpected = true;
            if (_saveMode)
            {
                _rollList.Add(Roll);
                SaveRolls();
            }

            Close();
        }

        /// <summary>
        /// Validation event for name text box. Prevents the name from being set to an existing name.
        /// </summary>
        /// <param name="sender"></param>
        /// <param name="e"></param>
        private void txtName_Validating(object sender, System.ComponentModel.CancelEventArgs e)
        {
            if (_rollList.Any(r => r.Name == txtName.Text))
                e.Cancel = true;
        }

        /// <summary>
        /// Event handler for the roll dropbown box change handler.
        /// </summary>
        private void cmbRollName_TextChanged(object sender, EventArgs e)
        {
            if (cmbRollName.Name == string.Empty)
            {
                Roll = new DiceRoll();
                Roll.Clear();
                RefreshView();

                btnOk.Enabled = false;
                btnRemove.Enabled = false;
                return;
            }


            var newroll = _rollList.FirstOrDefault(d => d.Name == cmbRollName.Text);
            if (newroll == null)
                return;

            Roll = newroll;

            RefreshView();

            btnOk.Enabled = true;
            btnRemove.Enabled = true;
        }

        /// <summary>
        /// Event handler for combo box validation. Prevents the user from putting invalid names in the dropdown.
        /// </summary>
        private void cmbRollName_Validating(object sender, System.ComponentModel.CancelEventArgs e)
        {
            if (cmbRollName.Text == string.Empty)
                return;

            if (_rollList.All(r => r.Name != cmbRollName.Text))
                e.Cancel = true;
        }

        /// <summary>
        /// Remove button event handler.
        /// </summary>
        private void btnRemove_Click(object sender, EventArgs e)
        {
            _rollList.RemoveAll(r => r.Name == Roll.Name);
            Roll.Clear();

            SaveRolls();
            LoadRolls();

            RefreshView();

            btnOk.Enabled = false;
            btnRemove.Enabled = false;
        }
    }
}
