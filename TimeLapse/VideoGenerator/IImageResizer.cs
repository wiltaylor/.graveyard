
using System;
using System.Collections.Generic;
using System.Linq;
using System.Text;
using System.Threading.Tasks;

namespace VideoGenerator
{
    public interface IImageResizer
    {
        void SetFolders(string source, string destination);
        void Process(int screenCount, int[] screenOrder);
        void SetResolution(int width, int height);
    }
}
