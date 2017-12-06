using System;
using System.ComponentModel;
using System.Linq;
using System.Threading.Tasks;
using System.Windows.Forms;

namespace VideoGenerator
{
    public partial class frmMain : Form
    {
        public frmMain()
        {
            InitializeComponent();
        }

        private void DisableUI()
        {
            grpResize.Enabled = false;
            grpVideo.Enabled = false;
            grpAudio.Enabled = false;
            pbartimer.Enabled = true;
            progBar.Visible = true;
        }

        private void EnableUI()
        {
            grpResize.Enabled = true;
            grpVideo.Enabled = true;
            grpAudio.Enabled = true;
            pbartimer.Enabled = false;
            progBar.Visible = false;
        }

        private void frmMain_Load(object sender, EventArgs e)
        {
            //Prevent user from typing custom values in drop downs.
            cmbResizeResolution.DropDownStyle = ComboBoxStyle.DropDownList;
            cmbVideoResolution.DropDownStyle = ComboBoxStyle.DropDownList;

            EnableUI();
        }

        private void txtVideoFPS_Validating(object sender, CancelEventArgs e)
        {
            //If text is not a number or non positive fail validation.
            if (int.TryParse(txtVideoFPS.Text, out var n))
            {
                if (n > 0)
                    return;
            }

            e.Cancel = true;
        }

        private void BrowseForFolder(string text, TextBox textbox, bool showNewFolder)
        {
            var openFolder = new FolderBrowserDialog
            {
                Description = text,
                ShowNewFolderButton = showNewFolder
            };

            if (openFolder.ShowDialog() != DialogResult.OK)
                return;

            textbox.Text = openFolder.SelectedPath;
        }

        private void BrowseForFile(string type, TextBox textbox, bool open)
        {
            if (!open)
            {
                var fsd = new SaveFileDialog {Filter = type};

                if (fsd.ShowDialog() != DialogResult.OK)
                    return;

                textbox.Text = fsd.FileName;
            }
            else
            {
                var fod = new OpenFileDialog {Filter = type};

                if (fod.ShowDialog() != DialogResult.OK)
                    return;

                textbox.Text = fod.FileName;
            }
        }

        private void btnResizeSource_Click(object sender, EventArgs e)
        {
            BrowseForFolder("Please select folder with screen captures created by the Capture Tool", txtResizeSource, false);
        }

        private void btnResizeDest_Click(object sender, EventArgs e)
        {
            BrowseForFolder("Please select destination for resized images", txtResizeDest, true);
        }

        private void btnVideoSource_Click(object sender, EventArgs e)
        {
            BrowseForFolder("Please select resized images to be converted into a video", txtVideoSource, false);
        }

        private void btnVideoDest_Click(object sender, EventArgs e)
        {
            BrowseForFile("MP4|*.mp4|Video|*.avi", txtVideoDest, false);
        }

        private void btnAudioSource_Click(object sender, EventArgs e)
        {
            BrowseForFile("Video|*.avi;*.mp4", txtAudioSource, true);
        }

        private void btnAudio_Click(object sender, EventArgs e)
        {
            BrowseForFile("Audio Files|*.mp3;*.ogg;*.wma| All Files|*.*", txtAudio, true);
        }

        private void btnAudioDest_Click(object sender, EventArgs e)
        {
            BrowseForFile("MP4|*.mp4|Video|*.avi", txtAudioDest, false);

        }

        private async void btnResize_Click(object sender, EventArgs e)
        {
            DisableUI();

            var source = txtResizeSource.Text;
            var dest = txtResizeDest.Text;
            var resParts = cmbResizeResolution.Text.Split('x').Select(int.Parse).ToArray();
            var screenOrder = txtScreenOrder.Text.Split(',').Select(int.Parse).ToArray();
            var gridChecked = rdoGrid.Checked;

            await Task.Run(() =>
            {
                var resizer = default(IImageResizer);

                if (gridChecked)
                    resizer = new GridResize();

                if (resizer == null)
                    return;

                resizer.SetFolders(source, dest);
                resizer.SetResolution(resParts[0], resParts[1]);
                resizer.Process(screenOrder.Length, screenOrder);
            });

            EnableUI();
        }

        private void rdoGrid_CheckedChanged(object sender, EventArgs e)
        {

        }

        private async void btnVideoBuild_Click(object sender, EventArgs e)
        {
            var resParts = cmbVideoResolution.Text.Split('x').Select(int.Parse).ToArray();
            var source = txtVideoSource.Text;
            var dest = txtVideoDest.Text;
            var fps = int.Parse(txtVideoFPS.Text);

            DisableUI();

            await Task.Run(() =>
            {
                var coder = new Mencoder();

                coder.GenerateVideo(source, dest, resParts[0], resParts[1],
                    fps);
            });

            EnableUI();

        }

        private async void btnAudioBuild_Click(object sender, EventArgs e)
        {
            DisableUI();

            var source = txtAudioSource.Text;
            var dest = txtAudioDest.Text;
            var audio = txtAudio.Text;

            await Task.Run(() =>
            {

                var coder = new Mencoder();

                coder.AddAudio(source, dest, audio);
            });

            EnableUI();
        }

        private void pbartimer_Tick(object sender, EventArgs e)
        {
            if (progBar.Value == progBar.Maximum)
                progBar.Value = progBar.Minimum;
            progBar.Value++;
        }
    }
}
