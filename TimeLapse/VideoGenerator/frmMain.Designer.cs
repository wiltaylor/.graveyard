namespace VideoGenerator
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
            this.components = new System.ComponentModel.Container();
            this.grpResize = new System.Windows.Forms.GroupBox();
            this.txtScreenOrder = new System.Windows.Forms.TextBox();
            this.label8 = new System.Windows.Forms.Label();
            this.btnResize = new System.Windows.Forms.Button();
            this.label3 = new System.Windows.Forms.Label();
            this.label2 = new System.Windows.Forms.Label();
            this.btnResizeDest = new System.Windows.Forms.Button();
            this.btnResizeSource = new System.Windows.Forms.Button();
            this.txtResizeSource = new System.Windows.Forms.TextBox();
            this.txtResizeDest = new System.Windows.Forms.TextBox();
            this.groupBox4 = new System.Windows.Forms.GroupBox();
            this.rdoGrid = new System.Windows.Forms.RadioButton();
            this.rdoLinedUp = new System.Windows.Forms.RadioButton();
            this.label1 = new System.Windows.Forms.Label();
            this.cmbResizeResolution = new System.Windows.Forms.ComboBox();
            this.grpVideo = new System.Windows.Forms.GroupBox();
            this.btnVideoBuild = new System.Windows.Forms.Button();
            this.txtVideoFPS = new System.Windows.Forms.TextBox();
            this.label7 = new System.Windows.Forms.Label();
            this.label5 = new System.Windows.Forms.Label();
            this.label4 = new System.Windows.Forms.Label();
            this.label6 = new System.Windows.Forms.Label();
            this.cmbVideoResolution = new System.Windows.Forms.ComboBox();
            this.btnVideoDest = new System.Windows.Forms.Button();
            this.txtVideoSource = new System.Windows.Forms.TextBox();
            this.btnVideoSource = new System.Windows.Forms.Button();
            this.txtVideoDest = new System.Windows.Forms.TextBox();
            this.grpAudio = new System.Windows.Forms.GroupBox();
            this.btnAudioBuild = new System.Windows.Forms.Button();
            this.label11 = new System.Windows.Forms.Label();
            this.txtAudioDest = new System.Windows.Forms.TextBox();
            this.btnAudioDest = new System.Windows.Forms.Button();
            this.label9 = new System.Windows.Forms.Label();
            this.label10 = new System.Windows.Forms.Label();
            this.txtAudio = new System.Windows.Forms.TextBox();
            this.btnAudio = new System.Windows.Forms.Button();
            this.btnAudioSource = new System.Windows.Forms.Button();
            this.txtAudioSource = new System.Windows.Forms.TextBox();
            this.progBar = new System.Windows.Forms.ProgressBar();
            this.pbartimer = new System.Windows.Forms.Timer(this.components);
            this.grpResize.SuspendLayout();
            this.groupBox4.SuspendLayout();
            this.grpVideo.SuspendLayout();
            this.grpAudio.SuspendLayout();
            this.SuspendLayout();
            // 
            // grpResize
            // 
            this.grpResize.Controls.Add(this.txtScreenOrder);
            this.grpResize.Controls.Add(this.label8);
            this.grpResize.Controls.Add(this.btnResize);
            this.grpResize.Controls.Add(this.label3);
            this.grpResize.Controls.Add(this.label2);
            this.grpResize.Controls.Add(this.btnResizeDest);
            this.grpResize.Controls.Add(this.btnResizeSource);
            this.grpResize.Controls.Add(this.txtResizeSource);
            this.grpResize.Controls.Add(this.txtResizeDest);
            this.grpResize.Controls.Add(this.groupBox4);
            this.grpResize.Controls.Add(this.label1);
            this.grpResize.Controls.Add(this.cmbResizeResolution);
            this.grpResize.Location = new System.Drawing.Point(12, 12);
            this.grpResize.Name = "grpResize";
            this.grpResize.Size = new System.Drawing.Size(342, 348);
            this.grpResize.TabIndex = 0;
            this.grpResize.TabStop = false;
            this.grpResize.Text = "Resize Images";
            // 
            // txtScreenOrder
            // 
            this.txtScreenOrder.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.txtScreenOrder.Location = new System.Drawing.Point(132, 270);
            this.txtScreenOrder.Name = "txtScreenOrder";
            this.txtScreenOrder.Size = new System.Drawing.Size(142, 26);
            this.txtScreenOrder.TabIndex = 10;
            this.txtScreenOrder.Text = "0,1,2";
            // 
            // label8
            // 
            this.label8.AutoSize = true;
            this.label8.Location = new System.Drawing.Point(18, 270);
            this.label8.Name = "label8";
            this.label8.Size = new System.Drawing.Size(108, 20);
            this.label8.TabIndex = 9;
            this.label8.Text = "Screen Order:";
            // 
            // btnResize
            // 
            this.btnResize.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left)));
            this.btnResize.Location = new System.Drawing.Point(22, 304);
            this.btnResize.Name = "btnResize";
            this.btnResize.Size = new System.Drawing.Size(75, 29);
            this.btnResize.TabIndex = 3;
            this.btnResize.Text = "Resize";
            this.btnResize.UseVisualStyleBackColor = true;
            this.btnResize.Click += new System.EventHandler(this.btnResize_Click);
            // 
            // label3
            // 
            this.label3.AutoSize = true;
            this.label3.Location = new System.Drawing.Point(22, 231);
            this.label3.Name = "label3";
            this.label3.Size = new System.Drawing.Size(94, 20);
            this.label3.TabIndex = 8;
            this.label3.Text = "Destination:";
            // 
            // label2
            // 
            this.label2.AutoSize = true;
            this.label2.Location = new System.Drawing.Point(24, 187);
            this.label2.Name = "label2";
            this.label2.Size = new System.Drawing.Size(64, 20);
            this.label2.TabIndex = 7;
            this.label2.Text = "Source:";
            // 
            // btnResizeDest
            // 
            this.btnResizeDest.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.btnResizeDest.Location = new System.Drawing.Point(280, 219);
            this.btnResizeDest.Name = "btnResizeDest";
            this.btnResizeDest.Size = new System.Drawing.Size(38, 38);
            this.btnResizeDest.TabIndex = 5;
            this.btnResizeDest.Text = "...";
            this.btnResizeDest.UseVisualStyleBackColor = true;
            this.btnResizeDest.Click += new System.EventHandler(this.btnResizeDest_Click);
            // 
            // btnResizeSource
            // 
            this.btnResizeSource.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.btnResizeSource.Location = new System.Drawing.Point(280, 175);
            this.btnResizeSource.Name = "btnResizeSource";
            this.btnResizeSource.Size = new System.Drawing.Size(38, 38);
            this.btnResizeSource.TabIndex = 3;
            this.btnResizeSource.Text = "...";
            this.btnResizeSource.UseVisualStyleBackColor = true;
            this.btnResizeSource.Click += new System.EventHandler(this.btnResizeSource_Click);
            // 
            // txtResizeSource
            // 
            this.txtResizeSource.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.txtResizeSource.Location = new System.Drawing.Point(122, 181);
            this.txtResizeSource.Name = "txtResizeSource";
            this.txtResizeSource.Size = new System.Drawing.Size(152, 26);
            this.txtResizeSource.TabIndex = 3;
            // 
            // txtResizeDest
            // 
            this.txtResizeDest.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.txtResizeDest.Location = new System.Drawing.Point(122, 225);
            this.txtResizeDest.Name = "txtResizeDest";
            this.txtResizeDest.Size = new System.Drawing.Size(152, 26);
            this.txtResizeDest.TabIndex = 6;
            // 
            // groupBox4
            // 
            this.groupBox4.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.groupBox4.Controls.Add(this.rdoGrid);
            this.groupBox4.Controls.Add(this.rdoLinedUp);
            this.groupBox4.Location = new System.Drawing.Point(22, 69);
            this.groupBox4.Name = "groupBox4";
            this.groupBox4.Size = new System.Drawing.Size(296, 100);
            this.groupBox4.TabIndex = 4;
            this.groupBox4.TabStop = false;
            this.groupBox4.Text = "Packing Solution:";
            // 
            // rdoGrid
            // 
            this.rdoGrid.AutoSize = true;
            this.rdoGrid.Checked = true;
            this.rdoGrid.Location = new System.Drawing.Point(6, 55);
            this.rdoGrid.Name = "rdoGrid";
            this.rdoGrid.Size = new System.Drawing.Size(64, 24);
            this.rdoGrid.TabIndex = 1;
            this.rdoGrid.TabStop = true;
            this.rdoGrid.Text = "Grid";
            this.rdoGrid.UseVisualStyleBackColor = true;
            this.rdoGrid.CheckedChanged += new System.EventHandler(this.rdoGrid_CheckedChanged);
            // 
            // rdoLinedUp
            // 
            this.rdoLinedUp.AutoSize = true;
            this.rdoLinedUp.Enabled = false;
            this.rdoLinedUp.Location = new System.Drawing.Point(6, 25);
            this.rdoLinedUp.Name = "rdoLinedUp";
            this.rdoLinedUp.Size = new System.Drawing.Size(98, 24);
            this.rdoLinedUp.TabIndex = 0;
            this.rdoLinedUp.Text = "Lined Up";
            this.rdoLinedUp.UseVisualStyleBackColor = true;
            // 
            // label1
            // 
            this.label1.AutoSize = true;
            this.label1.Location = new System.Drawing.Point(18, 38);
            this.label1.Name = "label1";
            this.label1.Size = new System.Drawing.Size(89, 20);
            this.label1.TabIndex = 3;
            this.label1.Text = "Resolution:";
            // 
            // cmbResizeResolution
            // 
            this.cmbResizeResolution.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.cmbResizeResolution.FormattingEnabled = true;
            this.cmbResizeResolution.Items.AddRange(new object[] {
            "3840x2160",
            "2560x1440",
            "1920x1080",
            "1280x720",
            "854x480",
            "640x360",
            "426x240"});
            this.cmbResizeResolution.Location = new System.Drawing.Point(113, 35);
            this.cmbResizeResolution.Name = "cmbResizeResolution";
            this.cmbResizeResolution.Size = new System.Drawing.Size(205, 28);
            this.cmbResizeResolution.TabIndex = 0;
            this.cmbResizeResolution.Text = "3840x2160";
            // 
            // grpVideo
            // 
            this.grpVideo.Controls.Add(this.btnVideoBuild);
            this.grpVideo.Controls.Add(this.txtVideoFPS);
            this.grpVideo.Controls.Add(this.label7);
            this.grpVideo.Controls.Add(this.label5);
            this.grpVideo.Controls.Add(this.label4);
            this.grpVideo.Controls.Add(this.label6);
            this.grpVideo.Controls.Add(this.cmbVideoResolution);
            this.grpVideo.Controls.Add(this.btnVideoDest);
            this.grpVideo.Controls.Add(this.txtVideoSource);
            this.grpVideo.Controls.Add(this.btnVideoSource);
            this.grpVideo.Controls.Add(this.txtVideoDest);
            this.grpVideo.Location = new System.Drawing.Point(360, 12);
            this.grpVideo.Name = "grpVideo";
            this.grpVideo.Size = new System.Drawing.Size(342, 348);
            this.grpVideo.TabIndex = 1;
            this.grpVideo.TabStop = false;
            this.grpVideo.Text = "Generate Video";
            // 
            // btnVideoBuild
            // 
            this.btnVideoBuild.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left)));
            this.btnVideoBuild.Location = new System.Drawing.Point(8, 307);
            this.btnVideoBuild.Name = "btnVideoBuild";
            this.btnVideoBuild.Size = new System.Drawing.Size(75, 29);
            this.btnVideoBuild.TabIndex = 18;
            this.btnVideoBuild.Text = "Build";
            this.btnVideoBuild.UseVisualStyleBackColor = true;
            this.btnVideoBuild.Click += new System.EventHandler(this.btnVideoBuild_Click);
            // 
            // txtVideoFPS
            // 
            this.txtVideoFPS.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.txtVideoFPS.Location = new System.Drawing.Point(179, 172);
            this.txtVideoFPS.Name = "txtVideoFPS";
            this.txtVideoFPS.Size = new System.Drawing.Size(134, 26);
            this.txtVideoFPS.TabIndex = 16;
            this.txtVideoFPS.Text = "15";
            this.txtVideoFPS.Validating += new System.ComponentModel.CancelEventHandler(this.txtVideoFPS_Validating);
            // 
            // label7
            // 
            this.label7.AutoSize = true;
            this.label7.Location = new System.Drawing.Point(19, 175);
            this.label7.Name = "label7";
            this.label7.Size = new System.Drawing.Size(154, 20);
            this.label7.TabIndex = 15;
            this.label7.Text = "Frames Per Second:";
            // 
            // label5
            // 
            this.label5.AutoSize = true;
            this.label5.Location = new System.Drawing.Point(17, 128);
            this.label5.Name = "label5";
            this.label5.Size = new System.Drawing.Size(94, 20);
            this.label5.TabIndex = 14;
            this.label5.Text = "Destination:";
            // 
            // label4
            // 
            this.label4.AutoSize = true;
            this.label4.Location = new System.Drawing.Point(19, 38);
            this.label4.Name = "label4";
            this.label4.Size = new System.Drawing.Size(89, 20);
            this.label4.TabIndex = 10;
            this.label4.Text = "Resolution:";
            // 
            // label6
            // 
            this.label6.AutoSize = true;
            this.label6.Location = new System.Drawing.Point(19, 84);
            this.label6.Name = "label6";
            this.label6.Size = new System.Drawing.Size(64, 20);
            this.label6.TabIndex = 13;
            this.label6.Text = "Source:";
            // 
            // cmbVideoResolution
            // 
            this.cmbVideoResolution.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.cmbVideoResolution.FormattingEnabled = true;
            this.cmbVideoResolution.Items.AddRange(new object[] {
            "3840x2160",
            "2560x1440",
            "1920x1080",
            "1280x720",
            "854x480",
            "640x360",
            "426x240"});
            this.cmbVideoResolution.Location = new System.Drawing.Point(114, 35);
            this.cmbVideoResolution.Name = "cmbVideoResolution";
            this.cmbVideoResolution.Size = new System.Drawing.Size(205, 28);
            this.cmbVideoResolution.TabIndex = 9;
            this.cmbVideoResolution.Text = "3840x2160";
            // 
            // btnVideoDest
            // 
            this.btnVideoDest.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.btnVideoDest.Location = new System.Drawing.Point(275, 116);
            this.btnVideoDest.Name = "btnVideoDest";
            this.btnVideoDest.Size = new System.Drawing.Size(38, 38);
            this.btnVideoDest.TabIndex = 11;
            this.btnVideoDest.Text = "...";
            this.btnVideoDest.UseVisualStyleBackColor = true;
            this.btnVideoDest.Click += new System.EventHandler(this.btnVideoDest_Click);
            // 
            // txtVideoSource
            // 
            this.txtVideoSource.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.txtVideoSource.Location = new System.Drawing.Point(117, 78);
            this.txtVideoSource.Name = "txtVideoSource";
            this.txtVideoSource.Size = new System.Drawing.Size(152, 26);
            this.txtVideoSource.TabIndex = 10;
            // 
            // btnVideoSource
            // 
            this.btnVideoSource.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.btnVideoSource.Location = new System.Drawing.Point(275, 72);
            this.btnVideoSource.Name = "btnVideoSource";
            this.btnVideoSource.Size = new System.Drawing.Size(38, 38);
            this.btnVideoSource.TabIndex = 9;
            this.btnVideoSource.Text = "...";
            this.btnVideoSource.UseVisualStyleBackColor = true;
            this.btnVideoSource.Click += new System.EventHandler(this.btnVideoSource_Click);
            // 
            // txtVideoDest
            // 
            this.txtVideoDest.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.txtVideoDest.Location = new System.Drawing.Point(117, 122);
            this.txtVideoDest.Name = "txtVideoDest";
            this.txtVideoDest.Size = new System.Drawing.Size(152, 26);
            this.txtVideoDest.TabIndex = 12;
            // 
            // grpAudio
            // 
            this.grpAudio.Controls.Add(this.btnAudioBuild);
            this.grpAudio.Controls.Add(this.label11);
            this.grpAudio.Controls.Add(this.txtAudioDest);
            this.grpAudio.Controls.Add(this.btnAudioDest);
            this.grpAudio.Controls.Add(this.label9);
            this.grpAudio.Controls.Add(this.label10);
            this.grpAudio.Controls.Add(this.txtAudio);
            this.grpAudio.Controls.Add(this.btnAudio);
            this.grpAudio.Controls.Add(this.btnAudioSource);
            this.grpAudio.Controls.Add(this.txtAudioSource);
            this.grpAudio.Location = new System.Drawing.Point(708, 12);
            this.grpAudio.Name = "grpAudio";
            this.grpAudio.Size = new System.Drawing.Size(342, 348);
            this.grpAudio.TabIndex = 2;
            this.grpAudio.TabStop = false;
            this.grpAudio.Text = "Add Audio";
            // 
            // btnAudioBuild
            // 
            this.btnAudioBuild.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Bottom | System.Windows.Forms.AnchorStyles.Left)));
            this.btnAudioBuild.Location = new System.Drawing.Point(6, 307);
            this.btnAudioBuild.Name = "btnAudioBuild";
            this.btnAudioBuild.Size = new System.Drawing.Size(75, 29);
            this.btnAudioBuild.TabIndex = 19;
            this.btnAudioBuild.Text = "Build";
            this.btnAudioBuild.UseVisualStyleBackColor = true;
            this.btnAudioBuild.Click += new System.EventHandler(this.btnAudioBuild_Click);
            // 
            // label11
            // 
            this.label11.AutoSize = true;
            this.label11.Location = new System.Drawing.Point(4, 126);
            this.label11.Name = "label11";
            this.label11.Size = new System.Drawing.Size(94, 20);
            this.label11.TabIndex = 27;
            this.label11.Text = "Destination:";
            // 
            // txtAudioDest
            // 
            this.txtAudioDest.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.txtAudioDest.Location = new System.Drawing.Point(104, 120);
            this.txtAudioDest.Name = "txtAudioDest";
            this.txtAudioDest.Size = new System.Drawing.Size(152, 26);
            this.txtAudioDest.TabIndex = 26;
            // 
            // btnAudioDest
            // 
            this.btnAudioDest.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.btnAudioDest.Location = new System.Drawing.Point(262, 114);
            this.btnAudioDest.Name = "btnAudioDest";
            this.btnAudioDest.Size = new System.Drawing.Size(38, 38);
            this.btnAudioDest.TabIndex = 25;
            this.btnAudioDest.Text = "...";
            this.btnAudioDest.UseVisualStyleBackColor = true;
            this.btnAudioDest.Click += new System.EventHandler(this.btnAudioDest_Click);
            // 
            // label9
            // 
            this.label9.AutoSize = true;
            this.label9.Location = new System.Drawing.Point(4, 82);
            this.label9.Name = "label9";
            this.label9.Size = new System.Drawing.Size(54, 20);
            this.label9.TabIndex = 24;
            this.label9.Text = "Audio:";
            // 
            // label10
            // 
            this.label10.AutoSize = true;
            this.label10.Location = new System.Drawing.Point(6, 38);
            this.label10.Name = "label10";
            this.label10.Size = new System.Drawing.Size(64, 20);
            this.label10.TabIndex = 23;
            this.label10.Text = "Source:";
            // 
            // txtAudio
            // 
            this.txtAudio.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.txtAudio.Location = new System.Drawing.Point(104, 76);
            this.txtAudio.Name = "txtAudio";
            this.txtAudio.Size = new System.Drawing.Size(152, 26);
            this.txtAudio.TabIndex = 22;
            // 
            // btnAudio
            // 
            this.btnAudio.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.btnAudio.Location = new System.Drawing.Point(262, 70);
            this.btnAudio.Name = "btnAudio";
            this.btnAudio.Size = new System.Drawing.Size(38, 38);
            this.btnAudio.TabIndex = 21;
            this.btnAudio.Text = "...";
            this.btnAudio.UseVisualStyleBackColor = true;
            this.btnAudio.Click += new System.EventHandler(this.btnAudio_Click);
            // 
            // btnAudioSource
            // 
            this.btnAudioSource.Anchor = ((System.Windows.Forms.AnchorStyles)((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Right)));
            this.btnAudioSource.Location = new System.Drawing.Point(262, 26);
            this.btnAudioSource.Name = "btnAudioSource";
            this.btnAudioSource.Size = new System.Drawing.Size(38, 38);
            this.btnAudioSource.TabIndex = 19;
            this.btnAudioSource.Text = "...";
            this.btnAudioSource.UseVisualStyleBackColor = true;
            this.btnAudioSource.Click += new System.EventHandler(this.btnAudioSource_Click);
            // 
            // txtAudioSource
            // 
            this.txtAudioSource.Anchor = ((System.Windows.Forms.AnchorStyles)(((System.Windows.Forms.AnchorStyles.Top | System.Windows.Forms.AnchorStyles.Left) 
            | System.Windows.Forms.AnchorStyles.Right)));
            this.txtAudioSource.Location = new System.Drawing.Point(104, 32);
            this.txtAudioSource.Name = "txtAudioSource";
            this.txtAudioSource.Size = new System.Drawing.Size(152, 26);
            this.txtAudioSource.TabIndex = 20;
            // 
            // progBar
            // 
            this.progBar.Location = new System.Drawing.Point(25, 120);
            this.progBar.Name = "progBar";
            this.progBar.Size = new System.Drawing.Size(1002, 68);
            this.progBar.TabIndex = 19;
            this.progBar.Visible = false;
            // 
            // pbartimer
            // 
            this.pbartimer.Interval = 1000;
            this.pbartimer.Tick += new System.EventHandler(this.pbartimer_Tick);
            // 
            // frmMain
            // 
            this.AutoScaleDimensions = new System.Drawing.SizeF(9F, 20F);
            this.AutoScaleMode = System.Windows.Forms.AutoScaleMode.Font;
            this.ClientSize = new System.Drawing.Size(1060, 371);
            this.Controls.Add(this.progBar);
            this.Controls.Add(this.grpAudio);
            this.Controls.Add(this.grpVideo);
            this.Controls.Add(this.grpResize);
            this.FormBorderStyle = System.Windows.Forms.FormBorderStyle.FixedToolWindow;
            this.Name = "frmMain";
            this.Text = "Video Generator";
            this.Load += new System.EventHandler(this.frmMain_Load);
            this.grpResize.ResumeLayout(false);
            this.grpResize.PerformLayout();
            this.groupBox4.ResumeLayout(false);
            this.groupBox4.PerformLayout();
            this.grpVideo.ResumeLayout(false);
            this.grpVideo.PerformLayout();
            this.grpAudio.ResumeLayout(false);
            this.grpAudio.PerformLayout();
            this.ResumeLayout(false);

        }

        #endregion

        private System.Windows.Forms.GroupBox grpResize;
        private System.Windows.Forms.Button btnResize;
        private System.Windows.Forms.Label label3;
        private System.Windows.Forms.Label label2;
        private System.Windows.Forms.Button btnResizeDest;
        private System.Windows.Forms.Button btnResizeSource;
        private System.Windows.Forms.TextBox txtResizeSource;
        private System.Windows.Forms.TextBox txtResizeDest;
        private System.Windows.Forms.GroupBox groupBox4;
        private System.Windows.Forms.RadioButton rdoGrid;
        private System.Windows.Forms.RadioButton rdoLinedUp;
        private System.Windows.Forms.Label label1;
        private System.Windows.Forms.ComboBox cmbResizeResolution;
        private System.Windows.Forms.GroupBox grpVideo;
        private System.Windows.Forms.GroupBox grpAudio;
        private System.Windows.Forms.Button btnVideoBuild;
        private System.Windows.Forms.TextBox txtVideoFPS;
        private System.Windows.Forms.Label label7;
        private System.Windows.Forms.Label label5;
        private System.Windows.Forms.Label label4;
        private System.Windows.Forms.Label label6;
        private System.Windows.Forms.ComboBox cmbVideoResolution;
        private System.Windows.Forms.Button btnVideoDest;
        private System.Windows.Forms.TextBox txtVideoSource;
        private System.Windows.Forms.Button btnVideoSource;
        private System.Windows.Forms.TextBox txtVideoDest;
        private System.Windows.Forms.Button btnAudioBuild;
        private System.Windows.Forms.Label label11;
        private System.Windows.Forms.TextBox txtAudioDest;
        private System.Windows.Forms.Button btnAudioDest;
        private System.Windows.Forms.Label label9;
        private System.Windows.Forms.Label label10;
        private System.Windows.Forms.TextBox txtAudio;
        private System.Windows.Forms.Button btnAudio;
        private System.Windows.Forms.Button btnAudioSource;
        private System.Windows.Forms.TextBox txtAudioSource;
        private System.Windows.Forms.TextBox txtScreenOrder;
        private System.Windows.Forms.Label label8;
        private System.Windows.Forms.ProgressBar progBar;
        private System.Windows.Forms.Timer pbartimer;
    }
}

