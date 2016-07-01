namespace VMLib.Disk
{
    public enum DiskType
    {
        IDE,
        SCSI,
        SATA
    }

    public interface IDiskBuilder
    {
        void CreateDisk(string path, DiskType type, long size);

    }
}
