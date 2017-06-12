namespace AIEDemo.Winforms
{
    partial class frmRoll
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
            System.ComponentModel.ComponentResourceManager resources = new System.ComponentModel.ComponentResourceManager(typeof(frmRoll));
            this.lblName = new System.Windows.Forms.Label();
            this.cmbRollName = new System.Windows.Forms.ComboBox();
            this.gboxStats = new System.Windows.Forms.GroupBox();
            this.lblChar = new System.Windows.Forms.Label();
            this.txtChar = new System.Windows.Forms.TextBox();
            this.txtWis = new System.Windows.Forms.TextBox();
            this.txtInt = new System.Windows.Forms.TextBox();
            this.lblWis = new System.Windows.Forms.Label();
            this.txtDex = new System.Windows.Forms.TextBox();
            this.txtConst = new System.Windows.Forms.TextBox();
            this.txtStr = new System.Windows.Forms.TextBox();
            this.lblstr = new System.Windows.Forms.Label();
            this.lblInt = new System.Windows.Forms.Label();
            this.lblDex = new System.Windows.Forms.Label();
            this.lblConst = new System.Windows.Forms.Label();
            this.btnOk = new System.Windows.Forms.Button();
            this.btnCancel = new System.Windows.Forms.Button();
            this.txtName = new System.Windows.Forms.TextBox();
            this.btnRemove = new System.Windows.Forms.Button();
            this.gboxStats.SuspendLayout();
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
            // cmbRollName
            // 
            this.cmbRollName.FormattingEnabled = true;
            this.cmbRollName.Location = new System.Drawing.Point(56, 6);
            this.cmbRollName.Name = "cmbRollName";
            this.cmbRollName.Size = new System.Drawing.Size(234, 21);
            this.cmbRollName.TabIndex = 1;
            this.cmbRollName.Visible = false;
            this.cmbRollName.TextChanged += new System.EventHandler(this.cmbRollName_TextChanged);
            this.cmbRollName.Validating += new System.ComponentModel.CancelEventHandler(this.cmbRollName_Validating);
            // 
            // gboxStats
            // 
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
            this.gboxStats.Location = new System.Drawing.Point(15, 33);
            this.gboxStats.Name = "gboxStats";
            this.gboxStats.Size = new System.Drawing.Size(281, 183);
            this.gboxStats.TabIndex = 6;
            this.gboxStats.TabStop = false;
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
            // lblstr
            // 
            this.lblstr.AutoSize = true;
            this.lblstr.Location = new System.Drawing.Point(7, 26);
            this.lblstr.Name = "lblstr";
            this.lblstr.Size = new System.Drawing.Size(50, 13);
            this.lblstr.TabIndex = 4;
            this.lblstr.Text = "Strength:";
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
            // btnOk
            // 
            this.btnOk.Location = new System.Drawing.Point(15, 226);
            this.btnOk.Name = "btnOk";
            this.btnOk.Size = new System.Drawing.Size(75, 23);
            this.btnOk.TabIndex = 7;
            this.btnOk.Text = "Ok";
            this.btnOk.UseVisualStyleBackColor = true;
            this.btnOk.Click += new System.EventHandler(this.btnOk_Click);
            // 
            // btnCancel
            // 
            this.btnCancel.Location = new System.Drawing.Point(177, 226);
            this.btnCancel.Name = "btnCancel";
            this.btnCancel.Size = new System.Drawing.Size(75, 23);
            this.btnCancel.TabIndex = 8;
            this.btnCancel.Text = "Cancel";
            this.btnCancel.UseVisualStyleBackColor = true;
            this.btnCancel.Click += new System.EventHandler(this.btnCancel_Click);
            // 
            // txtName
            // 
            this.txtName.Location = new System.Drawing.Point(56, 6);
            this.txtName.Name = "txtName";
            this.txtName.Size = new System.Drawing.Size(240, 20);
            this.txtName.TabIndex = 9;
            this.txtName.TextChanged += new System.EventHandler(this.txtName_TextChanged);
            this.txtName.Validating += new System.ComponentModel.CancelEventHandler(this.txtName_Validating);
            // 
            // btnRemove
            // 
            this.btnRemove.Location = new System.Drawing.Point(96, 226);
            this.btnRemove.Name = "btnRemove";
            this.btnRemove.Size = new System.Drawing.Size(75, 23);
            this.btnRemove.TabIndex = 10;
            this.btnRemove.Text = "Remove";
            this.btnRemove.UseVisualStyleBackColor = true;
            this.btnRemove.Click += new System.EventHandler(this.btnRemove_Click);
            // 
            // frmRoll
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(6F, 13F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.ClientSize = new System.Drawing.Size(305, 261);
            this.Controls.Add(this.btnRemove);
            this.Controls.Add(this.txtName);
            this.Controls.Add(this.btnCancel);
            this.Controls.Add(this.btnOk);
            this.Controls.Add(this.gboxStats);
            this.Controls.Add(this.cmbRollName);
            this.Controls.Add(this.lblName);
            this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedToolWindow;
            this.Icon = ((System.Drawing.Icon)(resources.GetObject("$this.Icon")));
            this.Name = "frmRoll";
            this.Text = "Save/Load Roll";
            this.FormClosing += new System.Windows.Forms.FormClosingEventHandler(this.frmRoll_FormClosing);
            this.gboxStats.ResumeLayout(false);
            this.gboxStats.PerformLayout();
            this.ResumeLayout(false);
            this.PerformLayout();

        }

        #endregion

        private System.Windows.Forms.Label lblName;
        private System.Windows.Forms.ComboBox cmbRollName;
        private System.Windows.Forms.GroupBox gboxStats;
        private System.Windows.Forms.Label lblChar;
        private System.Windows.Forms.TextBox txtChar;
        private System.Windows.Forms.TextBox txtWis;
        private System.Windows.Forms.TextBox txtInt;
        private System.Windows.Forms.Label lblWis;
        private System.Windows.Forms.TextBox txtDex;
        private System.Windows.Forms.TextBox txtConst;
        private System.Windows.Forms.TextBox txtStr;
        private System.Windows.Forms.Label lblstr;
        private System.Windows.Forms.Label lblInt;
        private System.Windows.Forms.Label lblDex;
        private System.Windows.Forms.Label lblConst;
        private System.Windows.Forms.Button btnOk;
        private System.Windows.Forms.Button btnCancel;
        private System.Windows.Forms.TextBox txtName;
        private System.Windows.Forms.Button btnRemove;
    }
}