using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using System;
using System.Threading;
using System.Threading.Tasks;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace UFlix.Cross.Templates
{
    public sealed partial class IAPNotification : UserControl
    {
        private readonly SemaphoreSlim sema = new(1);
        public IAPNotification()
        {
            this.InitializeComponent();
        }

        public async void Show(string message)
        {
            await sema.WaitAsync();
            Visibility = Visibility.Visible;
            MText.Text = message;
            await Task.Delay(3000);

            Visibility = Visibility.Collapsed;
            sema.Release();
        }

        private void Button_Click(object sender, RoutedEventArgs e)
        {
            Visibility = Visibility.Collapsed;
            sema.Release();

        }
    }
}
