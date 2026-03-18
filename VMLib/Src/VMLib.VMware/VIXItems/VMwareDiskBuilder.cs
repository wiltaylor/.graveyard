using DiscUtils;
using DiscUtils.Ntfs;
using DiscUtils.Partitions;
using DiscUtils.Vmdk;
using VMLib.Disk;


namespace VMLib.VMware.VIXItems
{
    public class VMwareDiskBuilder :IDiskBuilder
    {
        public const int StreamBufferSize = 33554432;
        public void CreateDisk(string path, DiskType type, long size)
        {
            var disk = DiscUtils.Vmdk.Disk.Initialize(path, size, DiskCreateType.MonolithicSparse,
                    type == DiskType.IDE ? DiskAdapterType.Ide : DiskAdapterType.LsiLogicScsi);

            disk.Dispose();
        }

        public void CreateDisk(string path, DiskType type, long size, string folder)
        {
            var disk = DiscUtils.Vmdk.Disk.Initialize(path, size, DiskCreateType.MonolithicSparse,
                type == DiskType.IDE ? DiskAdapterType.Ide : DiskAdapterType.LsiLogicScsi);

            var part = new BiosPartitionTable(disk);
            var partindex = part.Create(WellKnownPartitionType.WindowsNtfs, true);
            var partition = part.Partitions[partindex];
            var partstream = partition.Open();

            var ntfs = NtfsFileSystem.Format(partstream, "", Geometry.FromCapacity(size), partition.FirstSector,
                partition.SectorCount);
            
            disk.Dispose();
        }
    }
}
