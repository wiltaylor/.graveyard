using Microsoft.AspNetCore.SignalR;

namespace InterviewDemoApp.Web.Hubs;

public class NumberHub: Hub<INumberClientHub>
{
    public async Task SetupIntervals(int interval)
    {
        await Clients.Caller.SendMessage($"Setup intervals for {interval} seconds.");
    }

    public async Task AddNumber(ulong number)
    {
        await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
    }

    public async Task Halt()
    {
        await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
    }

    public async Task Resume()
    {
        await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
    }

    public async Task Quit()
    {
        await Clients.Caller.SendMessage("Error - You need to call SetupIntervals first!");
    }
}