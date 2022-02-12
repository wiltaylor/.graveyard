using Microsoft.AspNetCore.SignalR;

namespace InterviewDemoApp.Web.Hubs;

public class NumberHub: Hub<INumberClientHub>
{
    public async Task SetupIntervals(int interval)
    {
        await Clients.Caller.SendMessage("Setup intervals");
    }

    public Task AddNumber(ulong number)
    {
        return Task.CompletedTask;
    }

    public Task Halt()
    {
        return Task.CompletedTask;
    }

    public Task Resume()
    {
        return Task.CompletedTask;
    }

    public Task Quit()
    {
        return Task.CompletedTask;
    }
}