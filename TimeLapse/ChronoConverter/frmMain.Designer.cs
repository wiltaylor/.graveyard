namespace ChronoConverter
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
            this.label1 = new System.Windows.Forms.Label();
            this.txtSource = new System.Windows.Forms.TextBox();
            this.txtDest = new System.Windows.Forms.TextBox();
            this.label2 = new System.Windows.Forms.Label();
            this.txtHeight = new System.Windows.Forms.TextBox();
            this.label3 = new System.Windows.Forms.Label();
            this.txtWidth = new System.Windows.Forms.TextBox();
            this.label4 = new System.Windows.Forms.Label();
            this.btnGo = new System.Windows.Forms.Button();
            this.txtScreenCount = new System.Windows.Forms.TextBox();
            this.label5 = new System.Windows.Forms.Label();
            this.progbar = new System.Windows.Forms.ProgressBar();
            this.btnSource = new System.Windows.Forms.Button();
            this.btnDest = new System.Windows.Forms.Button();
            this.SuspendLayout();
            // 
            // label1
            // 
            this.label1.AutoSize = true;
            this.label1.Location = new System.Drawing.Point(21, 23);
            this.label1.Name = "label1";
            this.label1.Size = new System.Drawing.Size(64, 20);
            this.label1.TabIndex = 0;
            this.label1.Text = "Source:";
            // 
            // txtSource
            // 
            this.txtSource.Location = new System.Drawing.Point(136, 17);
            this.txtSource.Name = "txtSource";
            this.txtSource.Size = new System.Drawing.Size(181, 26);
            this.txtSource.TabIndex = 1;
            // 
            // txtDest
            // 
            this.txtDest.Location = new System.Drawing.Point(136, 67);
            this.txtDest.Name = "txtDest";
            this.txtDest.Size = new System.Drawing.Size(181, 26);
            this.txtDest.TabIndex = 3;
            // 
            // label2
            // 
            this.label2.AutoSize = true;
            this.label2.Location = new System.Drawing.Point(21, 73);
            this.label2.Name = "label2";
            this.label2.Size = new System.Drawing.Size(94, 20);
            this.label2.TabIndex = 2;
            this.label2.Text = "Destination:";
            // 
            // txtHeight
            // 
            this.txtHeight.Location = new System.Drawing.Point(136, 160);
            this.txtHeight.Name = "txtHeight";
            this.txtHeight.Size = new System.Drawing.Size(181, 26);
            this.txtHeight.TabIndex = 7;
            this.txtHeight.Text = "2160";
            // 
            // label3
            // 
            this.label3.AutoSize = true;
            this.label3.Location = new System.Drawing.Point(21, 166);
            this.label3.Name = "label3";
            this.label3.Size = new System.Drawing.Size(115, 20);
            this.label3.TabIndex = 6;
            this.label3.Text = "Screen Height:";
            // 
            // txtWidth
            // 
            this.txtWidth.Location = new System.Drawing.Point(136, 110);
            this.txtWidth.Name = "txtWidth";
            this.txtWidth.Size = new System.Drawing.Size(181, 26);
            this.txtWidth.TabIndex = 5;
            this.txtWidth.Text = "3840";
            // 
            // label4
            // 
            this.label4.AutoSize = true;
            this.label4.Location = new System.Drawing.Point(21, 116);
            this.label4.Name = "label4";
            this.label4.Size = new System.Drawing.Size(109, 20);
            this.label4.TabIndex = 4;
            this.label4.Text = "Screen Width:";
            this.label4.Click += new System.EventHandler(this.label4_Click);
            // 
            // btnGo
            // 
            this.btnGo.Location = new System.Drawing.Point(25, 261);
            this.btnGo.Name = "btnGo";
            this.btnGo.Size = new System.Drawing.Size(292, 33);
            this.btnGo.TabIndex = 8;
            this.btnGo.Text = "Go";
            this.btnGo.UseVisualStyleBackColor = true;
            this.btnGo.Click += new System.EventHandler(this.button1_Click);
            // 
            // txtScreenCount
            // 
            this.txtScreenCount.Location = new System.Drawing.Point(136, 203);
            this.txtScreenCount.Name = "txtScreenCount";
            this.txtScreenCount.Size = new System.Drawing.Size(181, 26);
            this.txtScreenCount.TabIndex = 10;
            this.txtScreenCount.Text = "3";
            // 
            // label5
            // 
            this.label5.AutoSize = true;
            this.label5.Location = new System.Drawing.Point(21, 209);
            this.label5.Name = "label5";
            this.label5.Size = new System.Drawing.Size(111, 20);
            this.label5.TabIndex = 9;
            this.label5.Text = "Screen Count:";
            // 
            // progbar
            // 
            this.progbar.Location = new System.Drawing.Point(25, 319);
            this.progbar.Name = "progbar";
            this.progbar.Size = new System.Drawing.Size(292, 23);
            this.progbar.TabIndex = 11;
            // 
            // btnSource
            // 
            this.btnSource.Location = new System.Drawing.Point(323, 12);
            this.btnSource.Name = "btnSource";
            this.btnSource.Size = new System.Drawing.Size(46, 37);
            this.btnSource.TabIndex = 12;
            this.btnSource.Text = "...";
            this.btnSource.UseVisualStyleBackColor = true;
            this.btnSource.Click += new System.EventHandler(this.btnSource_Click);
            // 
            // btnDest
            // 
            this.btnDest.Location = new System.Drawing.Point(323, 62);
            this.btnDest.Name = "btnDest";
            this.btnDest.Size = new System.Drawing.Size(46, 37);
            this.btnDest.TabIndex = 13;
            this.btnDest.Text = "...";
            this.btnDest.UseVisualStyleBackColor = true;
            this.btnDest.Click += new System.EventHandler(this.btnDest_Click);
            // 
            // frmMain
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(9F, 20F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.ClientSize = new System.Drawing.Size(392, 391);
            this.Controls.Add(this.btnDest);
            this.Controls.Add(this.btnSource);
            this.Controls.Add(this.progbar);
            this.Controls.Add(this.txtScreenCount);
            this.Controls.Add(this.label5);
            this.Controls.Add(this.btnGo);
            this.Controls.Add(this.txtHeight);
            this.Controls.Add(this.label3);
            this.Controls.Add(this.txtWidth);
            this.Controls.Add(this.label4);
            this.Controls.Add(this.txtDest);
            this.Controls.Add(this.label2);
            this.Controls.Add(this.txtSource);
            this.Controls.Add(this.label1);
            this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedToolWindow;
            this.Name = "frmMain";
            this.Text = "Chronocast Converter";
            this.ResumeLayout(false);
            this.PerformLayout();

        }

        #endregion

        private System.Windows.Forms.Label label1;
        private System.Windows.Forms.TextBox txtSource;
        private System.Windows.Forms.TextBox txtDest;
        private System.Windows.Forms.Label label2;
        private System.Windows.Forms.TextBox txtHeight;
        private System.Windows.Forms.Label label3;
        private System.Windows.Forms.TextBox txtWidth;
        private System.Windows.Forms.Label label4;
        private System.Windows.Forms.Button btnGo;
        private System.Windows.Forms.TextBox txtScreenCount;
        private System.Windows.Forms.Label label5;
        private System.Windows.Forms.ProgressBar progbar;
        private System.Windows.Forms.Button btnSource;
        private System.Windows.Forms.Button btnDest;
    }
}

