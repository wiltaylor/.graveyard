namespace AIEDemo.Winforms
{
    partial class frmMain
    {
        /// <summary>
        /// Required designer variable.
        /// </summary>
        private System.ComponentModel.IContainer components = null;

        /// <summary>
        /// Clean up any resources being used.
        /// </summary>
        /// <param name="disposing">true if managed resources should be disposed; otherwise, false.</param>
        protected override void Dispose(bool disposing)
        {
            if (disposing && (components != null))
            {
                components.Dispose();
            }
            base.Dispose(disposing);
        }

        #region Windows Form Designer generated code

        /// <summary>
        /// Required method for Designer support - do not modify
        /// the contents of this method with the code editor.
        /// </summary>
        private void InitializeComponent()
        {
            System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(frmMain));
            this.lblName = new System.Windows.Forms.Label();
            this.txtName = new System.Windows.Forms.TextBox();
            this.lblClass = new System.Windows.Forms.Label();
            this.cmbClass = new System.Windows.Forms.ComboBox();
            this.lblstr = new System.Windows.Forms.Label();
            this.gboxStats = new System.Windows.Forms.GroupBox();
            this.btnLoadRoll = new System.Windows.Forms.Button();
            this.btnSaveRoll = new System.Windows.Forms.Button();
            this.btnRoll = new System.Windows.Forms.Button();
            this.lblChar = new System.Windows.Forms.Label();
            this.txtChar = new System.Windows.Forms.TextBox();
            this.txtWis = new System.Windows.Forms.TextBox();
            this.txtInt = new System.Windows.Forms.TextBox();
            this.lblWis = new System.Windows.Forms.Label();
            this.txtDex = new System.Windows.Forms.TextBox();
            this.txtConst = new System.Windows.Forms.TextBox();
            this.txtStr = new System.Windows.Forms.TextBox();
            this.lblInt = new System.Windows.Forms.Label();
            this.lblDex = new System.Windows.Forms.Label();
            this.lblConst = new System.Windows.Forms.Label();
            this.groupBox1 = new System.Windows.Forms.GroupBox();
            this.txtMana = new System.Windows.Forms.TextBox();
            this.lblMana = new System.Windows.Forms.Label();
            this.txtHP = new System.Windows.Forms.TextBox();
            this.lblHp = new System.Windows.Forms.Label();
            this.btnSave = new System.Windows.Forms.Button();
            this.btnLoad = new System.Windows.Forms.Button();
            this.lblSex = new System.Windows.Forms.Label();
            this.rdoMale = new System.Windows.Forms.RadioButton();
            this.rdoFemale = new System.Windows.Forms.RadioButton();
            this.rdoOther = new System.Windows.Forms.RadioButton();
            this.gboxStats.SuspendLayout();
            this.groupBox1.SuspendLayout();
            this.SuspendLayout();
            // 
            // lblName
            // 
            this.lblName.AutoSize = true;
            this.lblName.Location = new System.Drawing.Point(12, 9);
            this.lblName.Name = "lblName";
            this.lblName.Size = new System.Drawing.Size(38, 13);
            this.lblName.TabIndex = 0;
            this.lblName.Text = "Name:";
            // 
            // txtName
            // 
            this.txtName.Location = new System.Drawing.Point(113, 2);
            this.txtName.Name = "txtName";
            this.txtName.Size = new System.Drawing.Size(183, 20);
            this.txtName.TabIndex = 1;
            this.txtName.TextChanged += new System.EventHandler(this.ControlUpdated);
            // 
            // lblClass
            // 
            this.lblClass.AutoSize = true;
            this.lblClass.Location = new System.Drawing.Point(12, 34);
            this.lblClass.Name = "lblClass";
            this.lblClass.Size = new System.Drawing.Size(35, 13);
            this.lblClass.TabIndex = 2;
            this.lblClass.Text = "Class:";
            // 
            // cmbClass
            // 
            this.cmbClass.FormattingEnabled = true;
            this.cmbClass.Items.AddRange(new object[] {
            "Fighter",
            "Magic User"});
            this.cmbClass.Location = new System.Drawing.Point(113, 31);
            this.cmbClass.Name = "cmbClass";
            this.cmbClass.Size = new System.Drawing.Size(183, 21);
            this.cmbClass.TabIndex = 3;
            this.cmbClass.TextChanged += new System.EventHandler(this.ControlUpdated);
            this.cmbClass.Validating += new System.ComponentModel.CancelEventHandler(this.cmbClass_Validating);
            // 
            // lblstr
            // 
            this.lblstr.AutoSize = true;
            this.lblstr.Location = new System.Drawing.Point(7, 26);
            this.lblstr.Name = "lblstr";
            this.lblstr.Size = new System.Drawing.Size(50, 13);
            this.lblstr.TabIndex = 4;
            this.lblstr.Text = "Strength:";
            // 
            // gboxStats
            // 
            this.gboxStats.Controls.Add(this.btnLoadRoll);
            this.gboxStats.Controls.Add(this.btnSaveRoll);
            this.gboxStats.Controls.Add(this.btnRoll);
            this.gboxStats.Controls.Add(this.lblChar);
            this.gboxStats.Controls.Add(this.txtChar);
            this.gboxStats.Controls.Add(this.txtWis);
            this.gboxStats.Controls.Add(this.txtInt);
            this.gboxStats.Controls.Add(this.lblWis);
            this.gboxStats.Controls.Add(this.txtDex);
            this.gboxStats.Controls.Add(this.txtConst);
            this.gboxStats.Controls.Add(this.txtStr);
            this.gboxStats.Controls.Add(this.lblstr);
            this.gboxStats.Controls.Add(this.lblInt);
            this.gboxStats.Controls.Add(this.lblDex);
            this.gboxStats.Controls.Add(this.lblConst);
            this.gboxStats.Location = new System.Drawing.Point(15, 93);
            this.gboxStats.Name = "gboxStats";
            this.gboxStats.Size = new System.Drawing.Size(281, 183);
            this.gboxStats.TabIndex = 5;
            this.gboxStats.TabStop = false;
            // 
            // btnLoadRoll
            // 
            this.btnLoadRoll.Location = new System.Drawing.Point(200, 46);
            this.btnLoadRoll.Name = "btnLoadRoll";
            this.btnLoadRoll.Size = new System.Drawing.Size(75, 23);
            this.btnLoadRoll.TabIndex = 17;
            this.btnLoadRoll.Text = "Load Roll";
            this.btnLoadRoll.UseVisualStyleBackColor = true;
            this.btnLoadRoll.Click += new System.EventHandler(this.btnLoadRoll_Click);
            // 
            // btnSaveRoll
            // 
            this.btnSaveRoll.Location = new System.Drawing.Point(200, 16);
            this.btnSaveRoll.Name = "btnSaveRoll";
            this.btnSaveRoll.Size = new System.Drawing.Size(75, 23);
            this.btnSaveRoll.TabIndex = 16;
            this.btnSaveRoll.Text = "Save Roll";
            this.btnSaveRoll.UseVisualStyleBackColor = true;
            this.btnSaveRoll.Click += new System.EventHandler(this.btnSaveRoll_Click);
            // 
            // btnRoll
            // 
            this.btnRoll.Location = new System.Drawing.Point(200, 150);
            this.btnRoll.Name = "btnRoll";
            this.btnRoll.Size = new System.Drawing.Size(75, 23);
            this.btnRoll.TabIndex = 6;
            this.btnRoll.Text = "Roll";
            this.btnRoll.UseVisualStyleBackColor = true;
            this.btnRoll.Click += new System.EventHandler(this.btnRoll_Click);
            // 
            // lblChar
            // 
            this.lblChar.AutoSize = true;
            this.lblChar.Location = new System.Drawing.Point(7, 155);
            this.lblChar.Name = "lblChar";
            this.lblChar.Size = new System.Drawing.Size(53, 13);
            this.lblChar.TabIndex = 9;
            this.lblChar.Text = "Charisma:";
            // 
            // txtChar
            // 
            this.txtChar.Location = new System.Drawing.Point(98, 148);
            this.txtChar.Name = "txtChar";
            this.txtChar.ReadOnly = true;
            this.txtChar.Size = new System.Drawing.Size(52, 20);
            this.txtChar.TabIndex = 15;
            // 
            // txtWis
            // 
            this.txtWis.Location = new System.Drawing.Point(98, 122);
            this.txtWis.Name = "txtWis";
            this.txtWis.ReadOnly = true;
            this.txtWis.Size = new System.Drawing.Size(52, 20);
            this.txtWis.TabIndex = 14;
            // 
            // txtInt
            // 
            this.txtInt.Location = new System.Drawing.Point(98, 96);
            this.txtInt.Name = "txtInt";
            this.txtInt.ReadOnly = true;
            this.txtInt.Size = new System.Drawing.Size(52, 20);
            this.txtInt.TabIndex = 13;
            // 
            // lblWis
            // 
            this.lblWis.AutoSize = true;
            this.lblWis.Location = new System.Drawing.Point(7, 129);
            this.lblWis.Name = "lblWis";
            this.lblWis.Size = new System.Drawing.Size(48, 13);
            this.lblWis.TabIndex = 8;
            this.lblWis.Text = "Wisdom:";
            // 
            // txtDex
            // 
            this.txtDex.Location = new System.Drawing.Point(98, 70);
            this.txtDex.Name = "txtDex";
            this.txtDex.ReadOnly = true;
            this.txtDex.Size = new System.Drawing.Size(52, 20);
            this.txtDex.TabIndex = 12;
            // 
            // txtConst
            // 
            this.txtConst.Location = new System.Drawing.Point(98, 44);
            this.txtConst.Name = "txtConst";
            this.txtConst.ReadOnly = true;
            this.txtConst.Size = new System.Drawing.Size(52, 20);
            this.txtConst.TabIndex = 11;
            // 
            // txtStr
            // 
            this.txtStr.Location = new System.Drawing.Point(98, 18);
            this.txtStr.Name = "txtStr";
            this.txtStr.ReadOnly = true;
            this.txtStr.Size = new System.Drawing.Size(52, 20);
            this.txtStr.TabIndex = 10;
            // 
            // lblInt
            // 
            this.lblInt.AutoSize = true;
            this.lblInt.Location = new System.Drawing.Point(7, 103);
            this.lblInt.Name = "lblInt";
            this.lblInt.Size = new System.Drawing.Size(64, 13);
            this.lblInt.TabIndex = 5;
            this.lblInt.Text = "Intelligence:";
            // 
            // lblDex
            // 
            this.lblDex.AutoSize = true;
            this.lblDex.Location = new System.Drawing.Point(6, 77);
            this.lblDex.Name = "lblDex";
            this.lblDex.Size = new System.Drawing.Size(51, 13);
            this.lblDex.TabIndex = 7;
            this.lblDex.Text = "Dexterity:";
            // 
            // lblConst
            // 
            this.lblConst.AutoSize = true;
            this.lblConst.Location = new System.Drawing.Point(6, 51);
            this.lblConst.Name = "lblConst";
            this.lblConst.Size = new System.Drawing.Size(65, 13);
            this.lblConst.TabIndex = 6;
            this.lblConst.Text = "Constitution:";
            // 
            // groupBox1
            // 
            this.groupBox1.Controls.Add(this.txtMana);
            this.groupBox1.Controls.Add(this.lblMana);
            this.groupBox1.Controls.Add(this.txtHP);
            this.groupBox1.Controls.Add(this.lblHp);
            this.groupBox1.Location = new System.Drawing.Point(15, 282);
            this.groupBox1.Name = "groupBox1";
            this.groupBox1.Size = new System.Drawing.Size(281, 81);
            this.groupBox1.TabIndex = 6;
            this.groupBox1.TabStop = false;
            // 
            // txtMana
            // 
            this.txtMana.Location = new System.Drawing.Point(98, 50);
            this.txtMana.Name = "txtMana";
            this.txtMana.ReadOnly = true;
            this.txtMana.Size = new System.Drawing.Size(52, 20);
            this.txtMana.TabIndex = 20;
            // 
            // lblMana
            // 
            this.lblMana.AutoSize = true;
            this.lblMana.Location = new System.Drawing.Point(10, 57);
            this.lblMana.Name = "lblMana";
            this.lblMana.Size = new System.Drawing.Size(37, 13);
            this.lblMana.TabIndex = 19;
            this.lblMana.Text = "Mana:";
            // 
            // txtHP
            // 
            this.txtHP.Location = new System.Drawing.Point(98, 24);
            this.txtHP.Name = "txtHP";
            this.txtHP.ReadOnly = true;
            this.txtHP.Size = new System.Drawing.Size(52, 20);
            this.txtHP.TabIndex = 18;
            // 
            // lblHp
            // 
            this.lblHp.AutoSize = true;
            this.lblHp.Location = new System.Drawing.Point(10, 27);
            this.lblHp.Name = "lblHp";
            this.lblHp.Size = new System.Drawing.Size(25, 13);
            this.lblHp.TabIndex = 5;
            this.lblHp.Text = "HP:";
            // 
            // btnSave
            // 
            this.btnSave.Location = new System.Drawing.Point(15, 378);
            this.btnSave.Name = "btnSave";
            this.btnSave.Size = new System.Drawing.Size(126, 23);
            this.btnSave.TabIndex = 7;
            this.btnSave.Text = "Save Charecter";
            this.btnSave.UseVisualStyleBackColor = true;
            this.btnSave.Click += new System.EventHandler(this.btnSave_Click);
            // 
            // btnLoad
            // 
            this.btnLoad.Location = new System.Drawing.Point(170, 378);
            this.btnLoad.Name = "btnLoad";
            this.btnLoad.Size = new System.Drawing.Size(126, 23);
            this.btnLoad.TabIndex = 8;
            this.btnLoad.Text = "Load Charecter";
            this.btnLoad.UseVisualStyleBackColor = true;
            this.btnLoad.Click += new System.EventHandler(this.btnLoad_Click);
            // 
            // lblSex
            // 
            this.lblSex.AutoSize = true;
            this.lblSex.Location = new System.Drawing.Point(12, 60);
            this.lblSex.Name = "lblSex";
            this.lblSex.Size = new System.Drawing.Size(28, 13);
            this.lblSex.TabIndex = 12;
            this.lblSex.Text = "Sex:";
            // 
            // rdoMale
            // 
            this.rdoMale.AutoSize = true;
            this.rdoMale.Location = new System.Drawing.Point(113, 58);
            this.rdoMale.Name = "rdoMale";
            this.rdoMale.Size = new System.Drawing.Size(48, 17);
            this.rdoMale.TabIndex = 13;
            this.rdoMale.TabStop = true;
            this.rdoMale.Text = "Male";
            this.rdoMale.UseVisualStyleBackColor = true;
            this.rdoMale.CheckedChanged += new System.EventHandler(this.ControlUpdated);
            // 
            // rdoFemale
            // 
            this.rdoFemale.AutoSize = true;
            this.rdoFemale.Location = new System.Drawing.Point(167, 58);
            this.rdoFemale.Name = "rdoFemale";
            this.rdoFemale.Size = new System.Drawing.Size(59, 17);
            this.rdoFemale.TabIndex = 14;
            this.rdoFemale.TabStop = true;
            this.rdoFemale.Text = "Female";
            this.rdoFemale.UseVisualStyleBackColor = true;
            this.rdoFemale.CheckedChanged += new System.EventHandler(this.ControlUpdated);
            // 
            // rdoOther
            // 
            this.rdoOther.AutoSize = true;
            this.rdoOther.Location = new System.Drawing.Point(237, 58);
            this.rdoOther.Name = "rdoOther";
            this.rdoOther.Size = new System.Drawing.Size(51, 17);
            this.rdoOther.TabIndex = 15;
            this.rdoOther.TabStop = true;
            this.rdoOther.Text = "Other";
            this.rdoOther.UseVisualStyleBackColor = true;
            this.rdoOther.CheckedChanged += new System.EventHandler(this.ControlUpdated);
            // 
            // frmMain
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(6F, 13F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.ClientSize = new System.Drawing.Size(302, 410);
            this.Controls.Add(this.rdoOther);
            this.Controls.Add(this.rdoFemale);
            this.Controls.Add(this.rdoMale);
            this.Controls.Add(this.lblSex);
            this.Controls.Add(this.btnLoad);
            this.Controls.Add(this.btnSave);
            this.Controls.Add(this.groupBox1);
            this.Controls.Add(this.gboxStats);
            this.Controls.Add(this.cmbClass);
            this.Controls.Add(this.lblClass);
            this.Controls.Add(this.txtName);
            this.Controls.Add(this.lblName);
            this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedSingle;
            this.Icon = ((System.Drawing.Icon)(resources.GetObject("$this.Icon")));
            this.MaximizeBox = false;
            this.MinimizeBox = false;
            this.Name = "frmMain";
            this.Text = "Charecter Creator";
            this.gboxStats.ResumeLayout(false);
            this.gboxStats.PerformLayout();
            this.groupBox1.ResumeLayout(false);
            this.groupBox1.PerformLayout();
            this.ResumeLayout(false);
            this.PerformLayout();

        }

        #endregion

        private System.Windows.Forms.Label lblName;
        private System.Windows.Forms.TextBox txtName;
        private System.Windows.Forms.Label lblClass;
        private System.Windows.Forms.ComboBox cmbClass;
        private System.Windows.Forms.Label lblstr;
        private System.Windows.Forms.GroupBox gboxStats;
        private System.Windows.Forms.Button btnLoadRoll;
        private System.Windows.Forms.Button btnSaveRoll;
        private System.Windows.Forms.Button btnRoll;
        private System.Windows.Forms.Label lblChar;
        private System.Windows.Forms.TextBox txtChar;
        private System.Windows.Forms.TextBox txtWis;
        private System.Windows.Forms.TextBox txtInt;
        private System.Windows.Forms.Label lblWis;
        private System.Windows.Forms.TextBox txtDex;
        private System.Windows.Forms.TextBox txtConst;
        private System.Windows.Forms.TextBox txtStr;
        private System.Windows.Forms.Label lblInt;
        private System.Windows.Forms.Label lblDex;
        private System.Windows.Forms.Label lblConst;
        private System.Windows.Forms.GroupBox groupBox1;
        private System.Windows.Forms.TextBox txtMana;
        private System.Windows.Forms.Label lblMana;
        private System.Windows.Forms.TextBox txtHP;
        private System.Windows.Forms.Label lblHp;
        private System.Windows.Forms.Button btnSave;
        private System.Windows.Forms.Button btnLoad;
        private System.Windows.Forms.Label lblSex;
        private System.Windows.Forms.RadioButton rdoMale;
        private System.Windows.Forms.RadioButton rdoFemale;
        private System.Windows.Forms.RadioButton rdoOther;
    }
}

