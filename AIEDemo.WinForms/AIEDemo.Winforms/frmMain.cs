using System;
using System.IO;
using System.Windows.Forms;
using Newtonsoft.Json;

namespace AIEDemo.Winforms
{
    /// <summary>
    /// Main Form of the application that displays the charecter stats and allows the player to modify them.
    /// </summary>
    public partial class frmMain : Form
    {
        //Private variables.
        private Charecter _charecter;
        private bool _disableUpdate;

        /// <summary>
        /// Constructor.
        /// </summary>
        public frmMain()
        {
            InitializeComponent();

            _charecter = new Charecter();
            RefreshView();
        }

        /// <summary>
        /// RefreshView retrives values from the loaded charecter object and sets the form controls to them.
        /// </summary>
        private void RefreshView()
        {
            //Prevent controls from writing their values back to the charecter object while we are updating them.
            //If this is not done the initial charecter class is overwriten with the default values when the first field is updated.
            _disableUpdate = true;

            txtName.Text = _charecter.Name;
            txtStr.Text = _charecter.Strength.ToString();
            txtConst.Text = _charecter.Constitution.ToString();
            txtDex.Text = _charecter.Dexterity.ToString();
            txtInt.Text = _charecter.Intelligence.ToString();
            txtWis.Text = _charecter.Wisdom.ToString();
            txtChar.Text = _charecter.Charisma.ToString();
            txtHP.Text = _charecter.Hp.ToString();
            txtMana.Text = _charecter.Mana.ToString();

            switch (_charecter.Class)
            {
                case CharecterClass.Fighter:
                    cmbClass.Text = "Fighter";
                    break;
                case CharecterClass.MagicUser:
                    cmbClass.Text = "Magic User";
                    break;
                default:
                    throw new ArgumentOutOfRangeException();
            }

            switch (_charecter.Sex)
            {
                case CharecterSex.Male:
                    rdoMale.Checked = true;
                    break;
                case CharecterSex.Female:
                    rdoFemale.Checked = true;
                    break;
                case CharecterSex.Other:
                    rdoOther.Checked = true;
                    break;
                default:
                    throw new ArgumentOutOfRangeException();
            }

            _disableUpdate = false;
        }

        /// <summary>
        /// UpdateChareceter writes the values from form controls back to charecter class.
        /// </summary>
        private void UpdateCharecter()
        {
            _charecter.Name = txtName.Text;
            _charecter.Strength = int.Parse(txtStr.Text);
            _charecter.Constitution = int.Parse(txtConst.Text);
            _charecter.Dexterity = int.Parse(txtDex.Text);
            _charecter.Intelligence = int.Parse(txtInt.Text);
            _charecter.Wisdom = int.Parse(txtWis.Text);
            _charecter.Charisma = int.Parse(txtChar.Text);

            if(rdoMale.Checked)
                _charecter.Sex = CharecterSex.Male;
            if(rdoFemale.Checked)
                _charecter.Sex = CharecterSex.Female;
            if (rdoOther.Checked)
                _charecter.Sex = CharecterSex.Other;

            if(cmbClass.Text == "Fighter")
                _charecter.Class = CharecterClass.Fighter;
            if(cmbClass.Text == "Magic User")
                _charecter.Class = CharecterClass.MagicUser;

            RefreshView();
        }

        /// <summary>
        /// Event handler for all controls update events.
        /// </summary>
        private void ControlUpdated(object sender, EventArgs e)
        {
            if(!_disableUpdate)
                UpdateCharecter();
        }

        /// <summary>
        /// Validation event handler for Class dropdown. Prevents the user from selecting an invalid value.
        /// </summary>
        private void cmbClass_Validating(object sender, System.ComponentModel.CancelEventArgs e)
        {
            if (cmbClass.Text != "Fighter" && cmbClass.Text != "Magic User")
                e.Cancel = true;
        }

        /// <summary>
        /// Event handler for roll button. Creates a new set of chareceter stats.
        /// </summary>
        private void btnRoll_Click(object sender, EventArgs e)
        {
            var roll = new DiceRoll();

            _charecter.Strength = roll.Strength;
            _charecter.Constitution = roll.Constitution;
            _charecter.Dexterity = roll.Dexterity;
            _charecter.Intelligence = roll.Intelligence;
            _charecter.Wisdom = roll.Wisdom;
            _charecter.Charisma = roll.Charisma;

            RefreshView();
        }

        /// <summary>
        /// Event handler for Save Roll button.
        /// </summary>
        private void btnSaveRoll_Click(object sender, EventArgs e)
        {
            var window = new frmRoll();

            window.ShowSave(_charecter.GetDiceRoll());
        }

        /// <summary>
        /// Event handler for the Load Roll button.
        /// </summary>
        private void btnLoadRoll_Click(object sender, EventArgs e)
        {
            var window = new frmRoll();

            window.ShowLoad();

            if (window.WasCancelled)
                return;

            _charecter.SetFromDiceRoll(window.Roll);
            RefreshView();
        }

        /// <summary>
        /// Event handler for Save button.
        /// </summary>
        private void btnSave_Click(object sender, EventArgs e)
        {
            var dlg = new SaveFileDialog {Filter = "Charecter File(*.jsonchar)|*.jsonchar"};

            if (dlg.ShowDialog() != DialogResult.OK)
                return;

            File.WriteAllText(dlg.FileName, JsonConvert.SerializeObject(_charecter, Formatting.Indented));
        }

        /// <summary>
        /// Event handler for Load button.
        /// </summary>
        private void btnLoad_Click(object sender, EventArgs e)
        {
            var dlg = new OpenFileDialog();
            dlg.Filter = "Charecter File(*.jsonchar)|*.jsonchar";

            if (dlg.ShowDialog() != DialogResult.OK)
                return;

            _charecter = JsonConvert.DeserializeObject<Charecter>(File.ReadAllText(dlg.FileName));
            RefreshView();
        }
    }
}
