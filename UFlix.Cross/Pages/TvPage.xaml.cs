using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Controls.Primitives;
using Microsoft.UI.Xaml.Data;
using Microsoft.UI.Xaml.Input;
using Microsoft.UI.Xaml.Media;
using Microsoft.UI.Xaml.Navigation;
using System;
using System.Collections.Generic;
using System.Collections.ObjectModel;
using System.IO;
using System.Linq;
using System.Runtime.InteropServices.WindowsRuntime;
using System.Threading.Tasks;
using UFlix.Engine.Models;
using UFlix.Engine.Services;
using UFlix.Cross.Services;
using Windows.Foundation;
using Windows.Foundation.Collections;

// To learn more about WinUI, the WinUI project structure,
// and more about our project templates, see: http://aka.ms/winui-project-info.

namespace UFlix.Cross.Pages
{
    /// <summary>
    /// An empty page that can be used on its own or navigated to within a Frame.
    /// </summary>
    public sealed partial class TvPage : Page
    {
        private readonly TVService tVService = new();
        private ObservableCollection<MediaItem> AllItems = [];
        public TvPage()
        {
            InitializeComponent();
        }

        protected override async void OnNavigatedTo(NavigationEventArgs e)
        {
            base.OnNavigatedTo(e);
           
            if(AllItems.Count == 0)
            {
                var all = await tVService.GetTvs();
                foreach (var item in all)
                {
                    AllItems.Add(item);
                }
            }
           
        }

        private void GridView_ItemClick(object sender, ItemClickEventArgs e)
        {
            if (e.ClickedItem != null && e.ClickedItem is MediaItem item)
            {
                NavigationHelper.OpenPlayer(item);
            }
        }
    }
}
