using System;
using System.Collections.Generic;
using System.ComponentModel;
using System.Data;
using System.Drawing;
using System.Linq;
using System.Runtime.InteropServices;
using System.Text;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace CaptureTool
{
    public partial class frmMain : Form
    {
        private bool _tipShown = false;
        private ScreenCapture _screenCapture;
        public frmMain()
        {
            
            InitializeComponent();

            UIStopCapture();
        }

        private void UIStopCapture()
        {
            btnStop.Enabled = false;
            btnStart.Enabled = true;
            startToolStripMenuItem.Enabled = true;
            startToolStripMenuItem.Visible = true;
            stopToolStripMenuItem.Enabled = false;
            stopToolStripMenuItem.Visible = false;
            txtFrames.Enabled = true;
            txtPath.Enabled = true;
        }

        private void UIStartCapture()
        {
            btnStop.Enabled = true;
            btnStart.Enabled = false;
            startToolStripMenuItem.Enabled = false;
            startToolStripMenuItem.Visible = false;
            stopToolStripMenuItem.Enabled = true;
            stopToolStripMenuItem.Visible = true;
            txtFrames.Enabled = false;
            txtPath.Enabled = false;
        }

        private void btnBrowse_Click(object sender, EventArgs e)
        {
            var opendir = new FolderBrowserDialog();
            opendir.ShowNewFolderButton = true;
            opendir.Description = "Select folder to store time lapse images in.";

            var result = opendir.ShowDialog();

            if(result != DialogResult.OK)
                return;

            txtPath.Text = opendir.SelectedPath;

        }

        private void frmMain_FormClosing(object sender, FormClosingEventArgs e)
        {
            if (e.CloseReason == CloseReason.UserClosing)
            {
                e.Cancel = true;
                this.Hide();

                if (!_tipShown)
                {
                    notifyIcon.ShowBalloonTip(5, "Time Lapose Tool",
                        "Time lapse tool is still open and can be closed from the notification area when you are done.",
                        ToolTipIcon.Info);
                    _tipShown = true;
                }
            }
        }

        private void notifyIcon_DoubleClick(object sender, EventArgs e)
        {
            this.Show();
        }

        private void optionsToolStripMenuItem_Click(object sender, EventArgs e)
        {
            this.Show();
        }

        private void exitToolStripMenuItem_Click(object sender, EventArgs e)
        {
            Application.Exit();
        }

        private void btnStart_Click(object sender, EventArgs e)
        {
            
            UIStartCapture();
            _screenCapture = new ScreenCapture(txtPath.Text);
            this.Hide();
            _screenCapture.Snapshot();
            captureTimer.Interval = int.Parse(txtFrames.Text) * 1000;
            captureTimer.Enabled = true;
        }
        private void btnStop_Click(object sender, EventArgs e)
        {
            captureTimer.Enabled = false;
            UIStopCapture();
            _screenCapture = null;
        }

        private void txtFrames_Validating(object sender, CancelEventArgs e)
        {
            if (!int.TryParse(txtFrames.Text, out var n))
                e.Cancel = true;
        }

        private void txtFrames_TextChanged(object sender, EventArgs e)
        {
            captureTimer.Interval = int.Parse(txtFrames.Text) * 1000;
        }

        private void captureTimer_Tick(object sender, EventArgs e)
        {
            _screenCapture.Snapshot();
        }
    }
}
